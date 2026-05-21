//! Azure Blob Storage source for AmortizedCost billing exports.
//!
//! Configured via three environment variables:
//!   AZ_BILLING_BLOB_SERVICE_URL  e.g. https://eroadstaazurebilling.blob.core.windows.net/
//!   AZ_BILLING_CONTAINER_NAME    e.g. azurecostmanagement
//!   AZ_BILLING_BLOB_PREFIX       e.g. eroad/Cortex-amortized-cost
//!
//! Blob layout under the container:
//!   {prefix}/{YYYYMMDD-YYYYMMDD}/{run-id-guid}/manifest.json
//!   {prefix}/{YYYYMMDD-YYYYMMDD}/{run-id-guid}/part_0_0001.csv
//!   ...

use azure_core::http::Url;
use azure_identity::DeveloperToolsCredential;
use azure_storage_blob::{BlobContainerClient, BlobServiceClient, models::BlobContainerClientListBlobsOptions};
use futures::TryStreamExt;
use serde::Deserialize;
use std::error::Error;
use std::path::PathBuf;

use crate::bills::Bills;
use crate::cmd_parse::FilterOpts;

/// Configuration loaded from environment variables.
#[derive(Debug, Clone)]
pub struct BlobSourceConfig {
    pub blob_service_url: String,
    pub container_name: String,
    /// Blob prefix up to (but not including) the date-range segment,
    /// e.g. `eroad/Cortex-amortized-cost`
    pub prefix: String,
}

impl BlobSourceConfig {
    /// Returns `Some` when all three required env vars are present and non-empty.
    pub fn from_env() -> Option<Self> {
        let url = std::env::var("AZ_BILLING_BLOB_SERVICE_URL")
            .ok()
            .filter(|s| !s.is_empty())?;
        let container = std::env::var("AZ_BILLING_CONTAINER_NAME")
            .ok()
            .filter(|s| !s.is_empty())?;
        let prefix = std::env::var("AZ_BILLING_BLOB_PREFIX")
            .ok()
            .filter(|s| !s.is_empty())?;
        Some(Self {
            blob_service_url: url,
            container_name: container,
            prefix,
        })
    }
}

// ---------------------------------------------------------------------------
// Manifest JSON deserialization types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManifestBlob {
    blob_name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManifestRunInfo {
    end_date: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BlobManifest {
    blobs: Vec<ManifestBlob>,
    run_info: ManifestRunInfo,
}

// ---------------------------------------------------------------------------
// BlobSource
// ---------------------------------------------------------------------------

/// Azure Blob Storage client for AmortizedCost billing exports.
pub struct BlobSource {
    pub config: BlobSourceConfig,
    container_client: BlobContainerClient,
}

impl BlobSource {
    /// Constructs a `BlobSource` using `DeveloperToolsCredential` (chains `az login` /
    /// `azd login`). Works locally; production deployments should extend this with
    /// `ManagedIdentityCredential` / `WorkloadIdentityCredential` when needed.
    pub fn new(config: BlobSourceConfig) -> azure_core::Result<Self> {
        let service_url = Url::parse(&config.blob_service_url)?;
        let credential = DeveloperToolsCredential::new(None)?;
        let service_client = BlobServiceClient::new(service_url, Some(credential), None)?;
        let container_client = service_client.blob_container_client(&config.container_name);
        Ok(Self { config, container_client })
    }

    /// Lists all `YYYYMMDD-YYYYMMDD` date-range folder names available under the
    /// configured prefix by scanning for `manifest.json` blobs.
    ///
    /// Blob names are expected to look like:
    ///   `{prefix}/{date-range}/{run-id}/manifest.json`
    ///
    /// Returns a sorted, deduplicated list of date-range strings.
    pub async fn list_date_range_folders(&self) -> azure_core::Result<Vec<String>> {
        let list_prefix = format!("{}/", self.config.prefix);

        let options = BlobContainerClientListBlobsOptions {
            prefix: Some(list_prefix.clone()),
            ..Default::default()
        };
        let mut stream = self.container_client.list_blobs(Some(options))?;

        let mut date_ranges: Vec<String> = Vec::new();
        while let Some(blob) = stream.try_next().await? {
            let name = match blob.name {
                Some(n) => n,
                None => continue,
            };

            // Only care about manifest files to locate date-range folders.
            if !name.ends_with("/manifest.json") {
                continue;
            }

            // Strip "{prefix}/" to get "{date-range}/{run-id}/manifest.json".
            let relative = name
                .strip_prefix(&list_prefix)
                .unwrap_or(name.as_str());

            // The first path segment is the date-range folder.
            if let Some(date_range) = relative.split('/').next() {
                if is_date_range(date_range) && !date_ranges.contains(&date_range.to_string()) {
                    date_ranges.push(date_range.to_string());
                }
            }
        }

        date_ranges.sort();
        Ok(date_ranges)
    }

    /// Downloads the manifest for the billing month `YYYY-MM` by listing blobs
    /// with prefix `{prefix}/{YYYYMM}` and parsing the first `manifest.json` found.
    ///
    /// If a cached manifest already exists on disk **and** its `endDate` equals the
    /// last day of the month at `T00:00:00` (i.e. the export is complete and will not
    /// change), the cached copy is used and no network request is made.
    ///
    /// Returns an error if no manifest is found for the given month.
    async fn fetch_manifest_for_month(
        &self,
        year: u32,
        month: u32,
    ) -> Result<BlobManifest, Box<dyn Error + Send + Sync>> {
        // Try the local cache first.
        if let Some(manifest) = self.try_load_cached_manifest(year, month)? {
            if is_month_complete(year, month, &manifest.run_info.end_date) {
                log::info!(
                    "[blob] using cached manifest for {year}-{month:02} (endDate={})",
                    manifest.run_info.end_date
                );
                return Ok(manifest);
            }
        }

        // Build a prefix that matches the start of the date-range folder for this month,
        // e.g. `eroad/Cortex-amortized-cost/202605` matches `…/20260501-20260531/…`.
        let month_prefix = format!("{}/{}{:02}", self.config.prefix, year, month);
        let options = BlobContainerClientListBlobsOptions {
            prefix: Some(month_prefix.clone()),
            ..Default::default()
        };
        let mut stream = self.container_client.list_blobs(Some(options))?;

        while let Some(blob) = stream.try_next().await? {
            let name = match blob.name {
                Some(n) => n,
                None => continue,
            };
            if !name.ends_with("/manifest.json") {
                continue;
            }
            log::info!("[blob] downloading manifest: {name}");
            let bytes = self.download_blob(&name).await?;
            let manifest: BlobManifest = serde_json::from_slice(&bytes)?;
            log::info!(
                "[blob] manifest endDate={} blobs={}",
                manifest.run_info.end_date,
                manifest.blobs.len()
            );
            // Cache manifest to disk so it's visible alongside the CSVs.
            let local_manifest = self.local_path_for_blob(&name);
            if let Some(parent) = local_manifest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&local_manifest, &bytes)?;
            log::debug!("[blob] cached manifest → {}", local_manifest.display());
            return Ok(manifest);
        }

        Err(format!(
            "No manifest.json found in blob storage for {year}-{month:02} under prefix '{month_prefix}'"
        )
        .into())
    }

    /// Looks for a cached `manifest.json` under the local blob cache directory for
    /// the given month.  Returns `None` if nothing is found or parsing fails.
    fn try_load_cached_manifest(
        &self,
        year: u32,
        month: u32,
    ) -> Result<Option<BlobManifest>, Box<dyn Error + Send + Sync>> {
        // Local root: blob/{container}/{prefix}/
        let mut cache_base = PathBuf::from("blob");
        cache_base.push(&self.config.container_name);
        for segment in self.config.prefix.split('/') {
            if !segment.is_empty() {
                cache_base.push(segment);
            }
        }
        if !cache_base.exists() {
            return Ok(None);
        }

        let month_prefix = format!("{}{:02}", year, month);

        // Iterate date-range directories starting with YYYYMM (e.g. "20260401-20260430").
        for dr_entry in std::fs::read_dir(&cache_base)? {
            let dr_entry = dr_entry?;
            let dr_name = dr_entry.file_name();
            if !dr_name.to_string_lossy().starts_with(&month_prefix) {
                continue;
            }
            // Look one level deeper for the run-id directory containing manifest.json.
            let dr_path = dr_entry.path();
            if !dr_path.is_dir() {
                continue;
            }
            for run_entry in std::fs::read_dir(&dr_path)? {
                let manifest_path = run_entry?.path().join("manifest.json");
                if manifest_path.exists() {
                    let bytes = std::fs::read(&manifest_path)?;
                    let manifest: BlobManifest = serde_json::from_slice(&bytes)?;
                    return Ok(Some(manifest));
                }
            }
        }
        Ok(None)
    }

    /// Loads all billing part CSVs for the given month, using a local disk cache.
    ///
    /// The manifest is always fetched from blob (it is small and detects when the
    /// current-month export has been replaced).  Each CSV part is only downloaded
    /// if it is not already present on disk under `./blob/{container}/{blob_name}`.
    pub async fn load_bills_for_month(
        &self,
        year: u32,
        month: u32,
        filter_opts: &FilterOpts,
    ) -> Result<Bills, Box<dyn Error + Send + Sync>> {
        let manifest = self.fetch_manifest_for_month(year, month).await?;

        // Pass 1 — ensure every file listed in the manifest is on disk.
        for blob_info in &manifest.blobs {
            let local_path = self.local_path_for_blob(&blob_info.blob_name);
            if local_path.exists() {
                log::debug!("[blob] cache hit  → {}", local_path.display());
            } else {
                log::info!("[blob] downloading: {}", blob_info.blob_name);
                let bytes = self.download_blob(&blob_info.blob_name).await?;
                log::debug!("[blob] writing {} bytes → {}", bytes.len(), local_path.display());
                if let Some(parent) = local_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&local_path, &bytes)?;
            }
        }

        // Pass 2 — parse CSV parts into Bills.
        let mut merged: Option<Bills> = None;
        for blob_info in &manifest.blobs {
            if !blob_info.blob_name.ends_with(".csv") {
                continue;
            }
            let local_path = self.local_path_for_blob(&blob_info.blob_name);
            let mut part = Bills::default();
            part.parse_csv(&local_path, filter_opts)
                .map_err(|e| -> Box<dyn Error + Send + Sync> { e.to_string().into() })?;
            log::debug!("[blob] parsed {} entries from {}", part.len(), local_path.display());
            match merged.as_mut() {
                None => merged = Some(part),
                Some(existing) => existing.extend_with(part),
            }
        }

        let mut bills = merged.ok_or_else(|| -> Box<dyn Error + Send + Sync> {
            format!("Manifest for {year}-{month:02} contained no CSV parts").into()
        })?;
        // Override the short name with the canonical YYYY-MM label so display
        // headers show "2026-04" instead of the full blob path.
        bills.file_short_name = format!("{year}-{month:02}");
        Ok(bills)
    }

    /// Downloads a single blob by name and returns its bytes.
    async fn download_blob(&self, blob_name: &str) -> azure_core::Result<Vec<u8>> {
        let result = self
            .container_client
            .blob_client(blob_name)
            .download(None)
            .await?;
        let bytes = result.body.collect().await?;
        Ok(bytes.to_vec())
    }

    /// Maps a blob path (relative to the container) to a local cache path.
    ///
    /// e.g. blob `eroad/Cortex-amortized-cost/20260501-20260531/{run-id}/part_0_0001.csv`
    ///   → `./blob/azurecostmanagement/eroad/Cortex-amortized-cost/20260501-20260531/{run-id}/part_0_0001.csv`
    fn local_path_for_blob(&self, blob_name: &str) -> PathBuf {
        let mut path = PathBuf::from("blob");
        path.push(&self.config.container_name);
        for segment in blob_name.split('/') {
            if !segment.is_empty() {
                path.push(segment);
            }
        }
        path
    }
}

/// Returns `true` when `s` matches the `YYYYMMDD-YYYYMMDD` format (17 ASCII digits + dash).
fn is_date_range(s: &str) -> bool {
    s.len() == 17
        && s.as_bytes()[8] == b'-'
        && s[..8].bytes().all(|b| b.is_ascii_digit())
        && s[9..].bytes().all(|b| b.is_ascii_digit())
}

/// Returns `true` when `end_date` equals the last day of `YYYY-MM` at midnight,
/// indicating the billing export is complete and will not be updated.
/// e.g. year=2026, month=4, end_date="2026-04-30T00:00:00" → true
fn is_month_complete(year: u32, month: u32, end_date: &str) -> bool {
    let last_day = days_in_month(year, month);
    end_date == format!("{year}-{month:02}-{last_day:02}T00:00:00")
}

fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}

#[cfg(test)]
mod tests {
    use super::{is_date_range, is_month_complete};

    #[test]
    fn valid_date_range() {
        assert!(is_date_range("20240801-20240831"));
        assert!(is_date_range("20260501-20260531"));
    }

    #[test]
    fn invalid_date_range() {
        assert!(!is_date_range("202408010-20240831")); // too long
        assert!(!is_date_range("2024080-20240831")); // too short
        assert!(!is_date_range("20240801_20240831")); // wrong separator
        assert!(!is_date_range("7192093c-7f13-4753-a186-58c842877141")); // run-id guid
        assert!(!is_date_range("manifest.json"));
    }

    #[test]
    fn month_complete_detection() {
        assert!(is_month_complete(2026, 4, "2026-04-30T00:00:00"));
        assert!(is_month_complete(2026, 1, "2026-01-31T00:00:00"));
        assert!(is_month_complete(2024, 2, "2024-02-29T00:00:00")); // leap year
        assert!(is_month_complete(2025, 2, "2025-02-28T00:00:00")); // non-leap

        // Partial month (current month export) — should NOT be treated as complete.
        assert!(!is_month_complete(2026, 5, "2026-05-21T00:00:00"));
        // Wrong day.
        assert!(!is_month_complete(2026, 4, "2026-04-29T00:00:00"));
    }
}
