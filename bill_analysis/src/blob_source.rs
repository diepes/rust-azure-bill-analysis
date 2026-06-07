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
use azure_storage_blob::{
    BlobContainerClient, BlobServiceClient, models::BlobContainerClientListBlobsOptions,
};
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
        Ok(Self {
            config,
            container_client,
        })
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
            let relative = name.strip_prefix(&list_prefix).unwrap_or(name.as_str());

            // The first path segment is the date-range folder.
            if let Some(date_range) = relative.split('/').next()
                && is_date_range(date_range)
                && !date_ranges.contains(&date_range.to_string())
            {
                date_ranges.push(date_range.to_string());
            }
        }

        date_ranges.sort();
        Ok(date_ranges)
    }

    /// Downloads the manifest for the billing month `YYYY-MM` by listing blobs
    /// with prefix `{prefix}/{YYYYMM}` and parsing the first `manifest.json` found.
    ///
    /// If a **canonical** cached manifest already exists on disk at
    /// `blob/{container}/{prefix}/{YYYYMMDD-YYYYMMDD}/manifest.json` and its
    /// `endDate` equals the last day of the month (i.e. the export is complete and
    /// will not change), the cached copy is used and no network request is made.
    ///
    /// For in-progress months the blob is always consulted first; if blob access
    /// fails the canonical local manifest is used as a fallback provided all its
    /// CSV parts are already on disk.
    ///
    /// Returns an error if no manifest is found for the given month.
    async fn fetch_manifest_for_month(
        &self,
        year: u32,
        month: u32,
    ) -> Result<BlobManifest, Box<dyn Error + Send + Sync>> {
        // Try the local cache first — use it unconditionally when the month is
        // complete (export will never change again).
        if let Some(manifest) = self.try_load_cached_manifest(year, month)?
            && is_month_complete(year, month, &manifest.run_info.end_date)
        {
            log::info!(
                "[blob] using cached manifest for {year}-{month:02} (endDate={})",
                manifest.run_info.end_date
            );
            return Ok(manifest);
        }

        // Attempt to fetch the manifest from blob storage.
        match self.fetch_manifest_from_blob(year, month).await {
            Ok(manifest) => Ok(manifest),
            Err(blob_err) => {
                // Blob storage unreachable — fall back to the best local manifest
                // if all its CSV parts are already cached on disk.
                if let Ok(Some(manifest)) = self.try_load_cached_manifest(year, month)
                    && self.all_parts_cached(&manifest)
                {
                    log::warn!(
                        "[blob] blob unreachable for {year}-{month:02} ({}); \
                             falling back to local cache (endDate={})",
                        blob_err,
                        manifest.run_info.end_date
                    );
                    return Ok(manifest);
                }
                Err(blob_err)
            }
        }
    }

    /// Inner helper: fetches the manifest from blob storage and caches it to disk.
    ///
    /// The manifest is written to two locations:
    /// - The run-ID-specific path it lives at in blob storage (preserves history).
    /// - A **canonical** path directly inside the date-range folder, i.e.
    ///   `blob/{container}/{prefix}/{YYYYMMDD-YYYYMMDD}/manifest.json`.
    ///   This canonical file is what `try_load_cached_manifest` reads; it is
    ///   overwritten on every successful fetch so it always reflects the latest run.
    async fn fetch_manifest_from_blob(
        &self,
        year: u32,
        month: u32,
    ) -> Result<BlobManifest, Box<dyn Error + Send + Sync>> {
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

            // Write to the run-ID-specific path.
            let run_id_path = self.local_path_for_blob(&name);
            if let Some(parent) = run_id_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&run_id_path, &bytes)?;
            log::debug!(
                "[blob] cached manifest (run-id) → {}",
                run_id_path.display()
            );

            // Also write to the canonical date-range-level path so that
            // try_load_cached_manifest always finds the latest version in one place.
            // The date-range segment is the first path component after the prefix.
            let relative = name
                .strip_prefix(&format!("{}/", self.config.prefix))
                .unwrap_or(&name);
            if let Some(date_range) = relative.split('/').next()
                && is_date_range(date_range)
            {
                let canonical_blob = format!("{}/{}/manifest.json", self.config.prefix, date_range);
                let canonical_path = self.local_path_for_blob(&canonical_blob);
                if let Some(parent) = canonical_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&canonical_path, &bytes)?;
                log::debug!("[blob] canonical manifest → {}", canonical_path.display());
            }

            return Ok(manifest);
        }

        Err(format!(
            "No manifest.json found in blob storage for {year}-{month:02} under prefix '{month_prefix}'"
        )
        .into())
    }

    /// Looks for the canonical `manifest.json` at the date-range folder level under
    /// the local blob cache directory for the given month.
    ///
    /// The canonical file (`blob/{container}/{prefix}/{YYYYMMDD-YYYYMMDD}/manifest.json`)
    /// is written (and overwritten) by `fetch_manifest_from_blob` on every successful
    /// blob fetch, so it always reflects the latest downloaded run.
    ///
    /// Returns `None` if no canonical manifest is found on disk.
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

        // Look for a canonical manifest.json directly inside each date-range directory
        // that matches the month (e.g. "20260501-20260531").
        for dr_entry in std::fs::read_dir(&cache_base)? {
            let dr_entry = dr_entry?;
            let dr_name = dr_entry.file_name();
            let dr_name_str = dr_name.to_string_lossy();
            if !dr_name_str.starts_with(&month_prefix) || !is_date_range(&dr_name_str) {
                continue;
            }
            let manifest_path = dr_entry.path().join("manifest.json");
            if manifest_path.exists() {
                let bytes = std::fs::read(&manifest_path)?;
                let manifest: BlobManifest = serde_json::from_slice(&bytes)?;
                return Ok(Some(manifest));
            }
        }
        Ok(None)
    }

    /// Returns `true` when every CSV part listed in `manifest` is already present
    /// in the local blob cache.
    fn all_parts_cached(&self, manifest: &BlobManifest) -> bool {
        manifest
            .blobs
            .iter()
            .filter(|b| b.blob_name.ends_with(".csv"))
            .all(|b| self.local_path_for_blob(&b.blob_name).exists())
    }

    /// Loads all billing part CSVs for the given month, using a local disk cache.
    ///
    /// The manifest is fetched from blob storage so stale local copies are detected.
    /// If blob storage is unreachable and all CSV parts are already cached on disk,
    /// the most-recent local manifest is used as a fallback (with a warning log).
    /// Each CSV part is only downloaded if it is not already present on disk under
    /// `./blob/{container}/{blob_name}`.
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
                log::debug!(
                    "[blob] writing {} bytes → {}",
                    bytes.len(),
                    local_path.display()
                );
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
            log::debug!(
                "[blob] parsed {} entries from {}",
                part.len(),
                local_path.display()
            );
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
            if year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400)) {
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
    use super::{BlobManifest, ManifestBlob, ManifestRunInfo, is_date_range, is_month_complete};

    fn make_manifest(end_date: &str, blobs: Vec<&str>) -> BlobManifest {
        BlobManifest {
            blobs: blobs
                .into_iter()
                .map(|name| ManifestBlob {
                    blob_name: name.to_string(),
                })
                .collect(),
            run_info: ManifestRunInfo {
                end_date: end_date.to_string(),
            },
        }
    }

    // ---------------------------------------------------------------------------
    // try_load_cached_manifest: reads the canonical manifest at date-range level
    // ---------------------------------------------------------------------------

    #[test]
    fn canonical_manifest_is_loaded_directly() {
        let tmp = tempfile::tempdir().unwrap();
        let (container, prefix) = ("billing", "myorg/costs");
        let date_range = "20260501-20260531";
        let end_date = "2026-05-20T00:00:00";

        // Write the canonical manifest directly in the date-range directory
        // (as fetch_manifest_from_blob now does).
        let mut dr_dir = tmp.path().to_path_buf();
        dr_dir.push("blob");
        dr_dir.push(container);
        for seg in prefix.split('/') {
            if !seg.is_empty() {
                dr_dir.push(seg);
            }
        }
        dr_dir.push(date_range);
        std::fs::create_dir_all(&dr_dir).unwrap();
        let manifest_json = serde_json::json!({
            "blobs": [{"blobName": "myorg/costs/20260501-20260531/run-abc/part_0.csv"}],
            "runInfo": { "endDate": end_date }
        });
        std::fs::write(dr_dir.join("manifest.json"), manifest_json.to_string()).unwrap();

        // Simulate try_load_cached_manifest logic.
        let mut cache_base = tmp.path().to_path_buf();
        cache_base.push("blob");
        cache_base.push(container);
        for seg in prefix.split('/') {
            if !seg.is_empty() {
                cache_base.push(seg);
            }
        }

        let month_prefix = "202605";
        let mut found_end: Option<String> = None;
        for dr_entry in std::fs::read_dir(&cache_base).unwrap() {
            let dr_entry = dr_entry.unwrap();
            let dr_name = dr_entry.file_name();
            let dr_name_str = dr_name.to_string_lossy();
            if !dr_name_str.starts_with(month_prefix) || !is_date_range(&dr_name_str) {
                continue;
            }
            let manifest_path = dr_entry.path().join("manifest.json");
            if manifest_path.exists() {
                let v: serde_json::Value =
                    serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
                found_end = Some(v["runInfo"]["endDate"].as_str().unwrap().to_string());
            }
        }

        assert_eq!(found_end.as_deref(), Some(end_date));
    }

    /// Verifies that run-ID subdirectory manifests (old-style cache layout) are NOT
    /// picked up by the new canonical-manifest logic — the canonical file must be
    /// written directly in the date-range directory, not inside a run-ID subdir.
    #[test]
    fn run_id_manifest_not_returned_without_canonical() {
        let tmp = tempfile::tempdir().unwrap();
        let (container, prefix) = ("billing", "myorg/costs");

        // Write a manifest only inside a run-ID subdir (old layout, no canonical).
        let mut run_dir = tmp.path().to_path_buf();
        run_dir.extend(["blob", container]);
        for seg in prefix.split('/') {
            if !seg.is_empty() {
                run_dir.push(seg);
            }
        }
        run_dir.extend(["20260501-20260531", "run-abc"]);
        std::fs::create_dir_all(&run_dir).unwrap();
        std::fs::write(
            run_dir.join("manifest.json"),
            r#"{"blobs":[],"runInfo":{"endDate":"2026-05-20T00:00:00"}}"#,
        )
        .unwrap();

        // The canonical path (date-range dir, no run-ID) should NOT exist.
        let canonical = run_dir.parent().unwrap().join("manifest.json");
        assert!(
            !canonical.exists(),
            "canonical manifest should not exist yet"
        );

        // Simulate try_load_cached_manifest: it only checks date_range/manifest.json.
        let mut cache_base = tmp.path().to_path_buf();
        cache_base.extend(["blob", container]);
        for seg in prefix.split('/') {
            if !seg.is_empty() {
                cache_base.push(seg);
            }
        }

        let mut found = false;
        for dr_entry in std::fs::read_dir(&cache_base).unwrap() {
            let dr_entry = dr_entry.unwrap();
            let dr_name = dr_entry.file_name();
            let s = dr_name.to_string_lossy();
            if !s.starts_with("202605") || !is_date_range(&s) {
                continue;
            }
            if dr_entry.path().join("manifest.json").exists() {
                found = true;
            }
        }

        assert!(
            !found,
            "should not find a manifest when only run-ID copy exists"
        );
    }

    // ---------------------------------------------------------------------------
    // all_parts_cached helper logic
    // ---------------------------------------------------------------------------

    #[test]
    fn all_parts_cached_true_when_files_exist() {
        let tmp = tempfile::tempdir().unwrap();
        let manifest = make_manifest(
            "2026-05-20T00:00:00",
            vec![
                "myorg/costs/20260501-20260531/run/part_0.csv",
                "myorg/costs/20260501-20260531/run/part_1.csv",
            ],
        );
        // Write both CSV files into blob/billing/...
        for blob in &manifest.blobs {
            let mut p = tmp.path().to_path_buf();
            p.push("blob");
            p.push("billing");
            for seg in blob.blob_name.split('/') {
                if !seg.is_empty() {
                    p.push(seg);
                }
            }
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, b"x").unwrap();
        }
        let all_present = manifest
            .blobs
            .iter()
            .filter(|b| b.blob_name.ends_with(".csv"))
            .all(|b| {
                let mut p = tmp.path().to_path_buf();
                p.push("blob");
                p.push("billing");
                for seg in b.blob_name.split('/') {
                    if !seg.is_empty() {
                        p.push(seg);
                    }
                }
                p.exists()
            });
        assert!(all_present);
    }

    #[test]
    fn all_parts_cached_false_when_file_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let manifest = make_manifest(
            "2026-05-20T00:00:00",
            vec![
                "myorg/costs/20260501-20260531/run/part_0.csv",
                "myorg/costs/20260501-20260531/run/part_1.csv",
            ],
        );
        // Only write the first CSV.
        {
            let mut p = tmp.path().to_path_buf();
            p.push("blob");
            p.push("billing");
            for seg in manifest.blobs[0].blob_name.split('/') {
                if !seg.is_empty() {
                    p.push(seg);
                }
            }
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, b"x").unwrap();
        }
        let all_present = manifest
            .blobs
            .iter()
            .filter(|b| b.blob_name.ends_with(".csv"))
            .all(|b| {
                let mut p = tmp.path().to_path_buf();
                p.push("blob");
                p.push("billing");
                for seg in b.blob_name.split('/') {
                    if !seg.is_empty() {
                        p.push(seg);
                    }
                }
                p.exists()
            });
        assert!(!all_present);
    }

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
