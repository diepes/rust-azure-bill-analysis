use anyhow::{Context, Result};
use chrono::Datelike;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct Reservation {
    pub reservation_order_id: String,
    pub reservation_id: String,
    pub display_name: String,
    #[serde(rename = "SKU")]
    pub sku: String,
    pub quantity: u32,
    pub purchase_date: String,
    pub expiry_date: String,
    pub term: String,
    pub state: String,
    pub scope: String,
    pub instance_flexibility: Option<String>,
    #[serde(rename = "Type")]
    pub resource_type: String,
    pub billing_plan: String,
    pub region: String,
    #[serde(skip)]
    pub monthly_cost: Option<f64>,
}

/// Fetch all reservations, using cache if available for current month
pub fn fetch_all_reservations() -> Result<Vec<Reservation>> {
    fetch_all_reservations_internal(false)
}

/// Fetch all reservations, forcing a refresh from Azure (bypassing cache)
pub fn fetch_all_reservations_force_refresh() -> Result<Vec<Reservation>> {
    fetch_all_reservations_internal(true)
}

/// Internal function to fetch reservations with optional cache bypass
fn fetch_all_reservations_internal(force_refresh: bool) -> Result<Vec<Reservation>> {
    let cache_file = get_cache_filename();

    // Try to load from cache first (unless force refresh)
    if !force_refresh {
        if let Ok(cached_reservations) = load_from_cache(&cache_file) {
            println!(
                "Loaded {} reservations from cache: {}",
                cached_reservations.len(),
                cache_file
            );
            return Ok(cached_reservations);
        }
    }

    // Cache miss or force refresh - fetch from Azure
    if force_refresh {
        println!("Bypassing cache, fetching from Azure...");
    } else {
        println!("Cache not found or invalid, fetching from Azure...");
    }
    let reservations = fetch_reservations_from_azure()?;

    // Save to cache
    if let Err(e) = save_to_cache(&cache_file, &reservations) {
        eprintln!("Warning: Failed to save cache: {}", e);
    } else {
        println!(
            "Saved {} reservations to cache: {}",
            reservations.len(),
            cache_file
        );
    }

    Ok(reservations)
}

/// Get the cache filename for the current month
fn get_cache_filename() -> String {
    use chrono::Local;
    let now = Local::now();
    format!("cache_reservations_{}.json", now.format("%Y%m"))
}

/// Load reservations from cache file
fn load_from_cache(cache_file: &str) -> Result<Vec<Reservation>> {
    let path = Path::new(cache_file);

    if !path.exists() {
        anyhow::bail!("Cache file does not exist");
    }

    let contents = fs::read_to_string(path).context("Failed to read cache file")?;

    let reservations: Vec<Reservation> =
        serde_json::from_str(&contents).context("Failed to parse cache file")?;

    Ok(reservations)
}

/// Save reservations to cache file
fn save_to_cache(cache_file: &str, reservations: &[Reservation]) -> Result<()> {
    let json =
        serde_json::to_string_pretty(reservations).context("Failed to serialize reservations")?;

    fs::write(cache_file, json).context("Failed to write cache file")?;

    Ok(())
}

/// Fetch reservations directly from Azure (no cache)
fn fetch_reservations_from_azure() -> Result<Vec<Reservation>> {
    // Step 1: Get all reservation order IDs
    let order_ids = get_reservation_order_ids()?;

    println!("Found {} reservation orders to process", order_ids.len());

    let mut all_reservations = Vec::new();

    // Step 2: For each order ID, get the detailed reservations
    for (i, order_id) in order_ids.iter().enumerate() {
        eprint!(
            "\rProcessing order {}/{}: {}...",
            i + 1,
            order_ids.len(),
            &order_id[..8.min(order_id.len())]
        );

        match get_reservations_for_order(order_id) {
            Ok(mut reservations) => {
                all_reservations.append(&mut reservations);
            }
            Err(e) => {
                eprintln!(
                    "\nWarning: Failed to get reservations for order {}: {}",
                    order_id, e
                );
            }
        }
    }
    eprintln!(); // New line after progress

    Ok(all_reservations)
}

fn get_reservation_order_ids() -> Result<Vec<String>> {
    let output = Command::new("az")
        .args(&[
            "reservations",
            "reservation-order",
            "list",
            "--query",
            "[?provisioningState=='Succeeded' || provisioningState=='Active'].name",
            "-o",
            "tsv",
        ])
        .output()
        .context("Failed to execute az command. Make sure Azure CLI is installed.")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("az command failed: {}", stderr);
    }

    let stdout =
        String::from_utf8(output.stdout).context("Failed to parse az command output as UTF-8")?;

    let order_ids: Vec<String> = stdout
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    Ok(order_ids)
}

fn get_reservations_for_order(order_id: &str) -> Result<Vec<Reservation>> {
    let query = format!(
        r#"[].{{
            ReservationOrderId: '{}',
            ReservationId: name,
            DisplayName: properties.displayName,
            SKU: sku.name,
            Quantity: properties.quantity,
            PurchaseDate: properties.purchaseDate,
            ExpiryDate: properties.expiryDate,
            Term: properties.term,
            State: properties.displayProvisioningState,
            Scope: properties.appliedScopeType,
            InstanceFlexibility: properties.instanceFlexibility,
            Type: properties.reservedResourceType,
            BillingPlan: properties.billingPlan,
            Region: location
        }}"#,
        order_id
    );

    let output = Command::new("az")
        .args(&[
            "reservations",
            "reservation",
            "list",
            "--reservation-order-id",
            order_id,
            "--query",
            &query,
            "-o",
            "json",
        ])
        .output()
        .context("Failed to execute az reservations reservation list")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("az reservation list failed: {}", stderr);
    }

    let stdout = String::from_utf8(output.stdout)
        .context("Failed to parse reservation list output as UTF-8")?;

    let reservations: Vec<Reservation> =
        serde_json::from_str(&stdout).context("Failed to parse JSON from az command")?;

    Ok(reservations)
}

/// Fetch reservation costs from Cost Management API for the Management subscription
/// Returns a HashMap of ReservationId -> monthly cost
/// Queries the previous complete month for more accurate billing data
pub fn fetch_reservation_costs() -> Result<std::collections::HashMap<String, f64>> {
    // Try to get subscription ID from environment, fallback to Azure CLI
    let management_subscription_id = match std::env::var("MANAGEMENT_SUBSCRIPTION_ID") {
        Ok(id) => id,
        Err(_) => {
            println!("MANAGEMENT_SUBSCRIPTION_ID not found in .env, querying Azure CLI...");
            let output = Command::new("az")
                .args(&["account", "show", "--query", "id", "-o", "tsv"])
                .output()
                .context("Failed to execute 'az account show' command. Make sure Azure CLI is installed and you're logged in.")?;
            
            if !output.status.success() {
                anyhow::bail!("Failed to get subscription ID from Azure CLI. Please set MANAGEMENT_SUBSCRIPTION_ID in .env file.");
            }
            
            let id = String::from_utf8(output.stdout)
                .context("Failed to parse subscription ID")?
                .trim()
                .to_string();
            
            if id.is_empty() {
                anyhow::bail!("Azure CLI returned empty subscription ID. Please set MANAGEMENT_SUBSCRIPTION_ID in .env file.");
            }
            
            println!("Using subscription ID from Azure CLI: {}", id);
            id
        }
    };
    
    // Get previous month's date range for complete billing data
    let now = chrono::Local::now();
    let last_month = if now.month() == 1 {
        chrono::NaiveDate::from_ymd_opt(now.year() - 1, 12, 1).unwrap()
    } else {
        chrono::NaiveDate::from_ymd_opt(now.year(), now.month() - 1, 1).unwrap()
    };
    
    // Get last day of previous month
    let last_day = if last_month.month() == 12 {
        31
    } else {
        chrono::NaiveDate::from_ymd_opt(last_month.year(), last_month.month() + 1, 1)
            .unwrap()
            .pred_opt()
            .unwrap()
            .day()
    };
    
    let from_date = format!("{}-{:02}-01", last_month.year(), last_month.month());
    let to_date = format!("{}-{:02}-{:02}", last_month.year(), last_month.month(), last_day);
    
    let query_body = format!(r#"{{
        "type": "ActualCost",
        "timeframe": "Custom",
        "timePeriod": {{
            "from": "{}",
            "to": "{}"
        }},
        "dataset": {{
            "granularity": "None",
            "aggregation": {{
                "totalCost": {{
                    "name": "PreTaxCost",
                    "function": "Sum"
                }}
            }},
            "grouping": [
                {{"type": "Dimension", "name": "ReservationId"}},
                {{"type": "Dimension", "name": "ReservationName"}}
            ]
        }}
    }}"#, from_date, to_date);

    let uri = format!(
        "https://management.azure.com/subscriptions/{}/providers/Microsoft.CostManagement/query?api-version=2023-11-01",
        management_subscription_id
    );

    println!("Fetching reservation costs from Cost Management API...");

    let output = Command::new("az")
        .args(&[
            "rest",
            "--method", "post",
            "--uri", &uri,
            "--body", &query_body,
        ])
        .output()
        .context("Failed to execute az rest command")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Cost Management API query failed: {}", stderr);
    }

    let stdout = String::from_utf8(output.stdout)
        .context("Failed to parse cost management output as UTF-8")?;

    let response: serde_json::Value =
        serde_json::from_str(&stdout).context("Failed to parse JSON from cost management API")?;

    let mut costs = std::collections::HashMap::new();

    // Parse the response structure
    if let Some(rows) = response["properties"]["rows"].as_array() {
        for row in rows {
            if let Some(row_array) = row.as_array() {
                if row_array.len() >= 2 {
                    if let (Some(cost), Some(reservation_id)) = 
                        (row_array[0].as_f64(), row_array[1].as_str()) {
                        // Only store non-empty reservation IDs
                        if !reservation_id.is_empty() {
                            costs.insert(reservation_id.to_string(), cost);
                        }
                    }
                }
            }
        }
    }

    println!("Retrieved cost data for {} reservations", costs.len());
    Ok(costs)
}
