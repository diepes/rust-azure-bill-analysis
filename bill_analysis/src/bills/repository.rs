use std::{collections::HashMap, path::PathBuf, sync::Arc};

use tokio::sync::RwLock;

use crate::bills::Bills;
use crate::blob_source::BlobSource;
use crate::cmd_parse::FilterOpts;
use crate::find_files;

/// A caching repository that loads Azure billing data from local CSVs or blob
/// storage. Each unique `(year, month)` pair is loaded once and cached as an
/// `Arc<Bills>` — subsequent calls return the same allocation.
pub struct BillRepository {
    data_dir: PathBuf,
    blob: Option<Arc<BlobSource>>,
    cache: Arc<RwLock<HashMap<(u32, u32), Arc<Bills>>>>,
}

impl BillRepository {
    pub fn new(data_dir: PathBuf, blob: Option<Arc<BlobSource>>) -> Self {
        Self {
            data_dir,
            blob,
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Return bills for `year`/`month`. Loads from local CSV or blob on the
    /// first call; subsequent calls return the cached `Arc<Bills>`.
    pub async fn get(&self, year: u32, month: u32) -> Result<Arc<Bills>, String> {
        let month_str = format!("{year}-{month:02}");

        // Fast path: read lock
        {
            let cache = self.cache.read().await;
            if let Some(bills) = cache.get(&(year, month)) {
                log::debug!("[repo] cache HIT  {month_str}");
                return Ok(Arc::clone(bills));
            }
        }

        log::debug!("[repo] cache MISS {month_str} — loading...");

        // Try local CSV first.
        if let Some(csv_path) = find_files::find_bill_csv(&self.data_dir, &month_str) {
            let filter_opts = FilterOpts { case_sensitive: false };
            let mut bills = Bills::default();
            bills
                .parse_csv(&csv_path, &filter_opts)
                .map_err(|e| format!("Failed to parse '{:?}': {e}", csv_path))?;
            log::info!(
                "[repo] loaded {month_str} from local ({} rows)",
                bills.len()
            );
            let bills = Arc::new(bills);
            self.cache
                .write()
                .await
                .insert((year, month), Arc::clone(&bills));
            return Ok(bills);
        }

        // Fall back to blob.
        if let Some(blob) = &self.blob {
            let filter_opts = FilterOpts { case_sensitive: false };
            let bills = blob
                .load_bills_for_month(year, month, &filter_opts)
                .await
                .map_err(|e| {
                    let msg = format!("Blob load failed for '{month_str}': {e}");
                    log::error!("[repo] {msg}");
                    msg
                })?;
            log::info!(
                "[repo] loaded {month_str} from blob ({} rows)",
                bills.len()
            );
            let bills = Arc::new(bills);
            self.cache
                .write()
                .await
                .insert((year, month), Arc::clone(&bills));
            return Ok(bills);
        }

        Err(format!(
            "No billing file found for '{month_str}' in {:?} and no blob source configured",
            self.data_dir
        ))
    }

    /// Return sorted `"YYYY-MM"` strings for all months found locally in `data_dir`.
    pub async fn list_months(&self) -> Vec<String> {
        find_files::list_bill_months(&self.data_dir)
    }

    /// Return sorted `"YYYY-MM"` strings for all months available locally
    /// **and** in blob storage (when configured).
    ///
    /// Blob date-range folder names (`"YYYYMMDD-YYYYMMDD"`) are converted to
    /// `"YYYY-MM"` using the start date.  Failures from blob listing are logged
    /// and silently ignored so that local months are still returned.
    pub async fn list_months_including_blob(&self) -> Vec<String> {
        let mut months: std::collections::HashSet<String> =
            find_files::list_bill_months(&self.data_dir)
                .into_iter()
                .collect();

        if let Some(blob) = &self.blob {
            match blob.list_date_range_folders().await {
                Ok(folders) => {
                    for dr in folders {
                        if dr.len() >= 6 {
                            let ym = format!("{}-{}", &dr[..4], &dr[4..6]);
                            months.insert(ym);
                        }
                    }
                }
                Err(e) => log::warn!("[repo] blob list_date_range_folders failed: {e}"),
            }
        }

        let mut result: Vec<String> = months.into_iter().collect();
        result.sort();
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_test_csv(tmp: &tempfile::TempDir) {
        let dest = tmp.path().join("2024-03-Detail_test.csv");
        std::fs::copy("tests/azure_test_data_01.csv", dest)
            .expect("copy test CSV into tempdir");
    }

    #[tokio::test]
    async fn loads_bills_from_local_csv() {
        let tmp = tempfile::tempdir().unwrap();
        setup_test_csv(&tmp);
        let repo = BillRepository::new(tmp.path().to_path_buf(), None);
        let bills = repo.get(2024, 3).await.unwrap();
        assert_eq!(bills.len(), 8);
    }

    #[tokio::test]
    async fn get_caches_on_second_call() {
        let tmp = tempfile::tempdir().unwrap();
        setup_test_csv(&tmp);
        let repo = BillRepository::new(tmp.path().to_path_buf(), None);
        let first = repo.get(2024, 3).await.unwrap();
        let second = repo.get(2024, 3).await.unwrap();
        assert!(
            Arc::ptr_eq(&first, &second),
            "second call must return the same Arc (cached)"
        );
    }

    #[tokio::test]
    async fn get_errors_on_missing_month() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = BillRepository::new(tmp.path().to_path_buf(), None);
        let result = repo.get(2099, 1).await;
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert!(
            err.contains("2099-01"),
            "error should mention the month: {err}"
        );
    }

    #[tokio::test]
    async fn list_months_returns_sorted_strings() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("2024-12-data")).unwrap();
        std::fs::create_dir(tmp.path().join("2024-01-data")).unwrap();
        std::fs::create_dir(tmp.path().join("2024-03-data")).unwrap();
        let repo = BillRepository::new(tmp.path().to_path_buf(), None);
        let months = repo.list_months().await;
        assert_eq!(months, vec!["2024-01", "2024-03", "2024-12"]);
    }

    #[tokio::test]
    async fn list_months_including_blob_returns_local_when_no_blob() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("2024-05-data")).unwrap();
        std::fs::create_dir(tmp.path().join("2024-06-data")).unwrap();
        let repo = BillRepository::new(tmp.path().to_path_buf(), None);
        let months = repo.list_months_including_blob().await;
        assert_eq!(months, vec!["2024-05", "2024-06"]);
    }
}
