use anyhow::{Context, Result};
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
