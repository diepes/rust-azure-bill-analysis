mod reservation;

use anyhow::Result;
use chrono::Datelike;
use reservation::{Reservation, fetch_all_reservations, fetch_all_reservations_force_refresh};

fn main() -> Result<()> {
    // Check for --refresh or --force flag
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_help();
        return Ok(());
    }

    let force_refresh = args
        .iter()
        .any(|arg| arg == "--refresh" || arg == "--force" || arg == "-f");
    let show_expired = args
        .iter()
        .any(|arg| arg == "--show-expired" || arg == "--show-expired-reservations");

    if force_refresh {
        println!("Force refresh enabled - bypassing cache");
    }

    println!("Fetching all active Azure reservations...");

    let mut reservations = if force_refresh {
        fetch_all_reservations_force_refresh()?
    } else {
        fetch_all_reservations()?
    };

    // Filter out expired reservations unless --show-expired flag is set
    let total_count = reservations.len();
    if !show_expired {
        reservations.retain(|r| !is_expired(&r.state));
        let filtered_count = total_count - reservations.len();
        if filtered_count > 0 {
            println!("Filtered out {} expired reservation(s)", filtered_count);
        }
    }

    println!("\nFound {} active reservations\n", reservations.len());
    if show_expired && total_count != reservations.len() {
        println!("(Showing all reservations including expired)\n");
    }

    // Save to JSON file first
    let json_output = serde_json::to_string_pretty(&reservations)?;
    std::fs::write("all_reservations.json", json_output)?;
    println!("Saved to all_reservations.json\n");

    println!("\n\nDetailed Reservations:");
    println!("{}", "=".repeat(160));

    // Print in table format
    println!(
        "{:<10} {:<40} {:<25} {:<5} {:<15} {:<12} {:<12} {:<6} {:<10} {:<5}",
        "OrderId",
        "DisplayName",
        "SKU",
        "Qty",
        "Type",
        "Purchase",
        "Expiry",
        "Term",
        "State",
        "Flex"
    );
    println!("{}", "-".repeat(160));

    for res in &reservations {
        let order_id_short = &res.reservation_order_id[..8.min(res.reservation_order_id.len())];
        let flex = res.instance_flexibility.as_deref().unwrap_or("N/A");
        println!(
            "{:<10} {:<40} {:<25} {:<5} {:<15} {:<12} {:<12} {:<6} {:<10} {:<5}",
            order_id_short,
            res.display_name,
            res.sku,
            res.quantity,
            res.resource_type,
            res.purchase_date,
            res.expiry_date,
            res.term,
            res.state,
            flex
        );
    }
    println!("{}", "=".repeat(160));
    println!();

    // Print summary statistics
    print_summary(&reservations);

    Ok(())
}

fn print_help() {
    println!("Azure Reservation Plan Tool");
    println!("\nUsage:");
    println!("  reservation_plan [OPTIONS]");
    println!("\nOptions:");
    println!("  -h, --help                        Show this help message");
    println!("  -f, --force, --refresh            Force refresh from Azure (bypass cache)");
    println!("  --show-expired-reservations       Include expired reservations in output");
    println!("\nDescription:");
    println!("  Fetches all active Azure reservations and displays summary information.");
    println!("  Results are cached in cache_reservations_YYYYMM.json for the current month.");
    println!("  By default, expired reservations are filtered out.");
    println!("  Use --force to bypass cache and fetch fresh data from Azure.");
}

fn is_expired(state: &str) -> bool {
    state.eq_ignore_ascii_case("Expired") || state.eq_ignore_ascii_case("Cancelled")
}

fn print_summary(reservations: &[Reservation]) {
    use std::collections::HashMap;

    let mut by_type: HashMap<String, (u32, u32)> = HashMap::new(); // (total_quantity, reservation_count)
    let mut by_vm_type: HashMap<String, (u32, u32)> = HashMap::new(); // VM types breakdown
    let mut by_term: HashMap<String, u32> = HashMap::new();
    let mut by_expiry_month: HashMap<String, (u32, u32)> = HashMap::new(); // (units, reservation_count)
    let mut by_expiry_month_3y: HashMap<String, (u32, u32)> = HashMap::new(); // 3-year term only
    let mut by_type_month: HashMap<(String, String), (u32, u32)> = HashMap::new(); // (resource_type, month) -> (units, count)
    let mut total_quantity = 0;

    for res in reservations {
        // Track by resource type
        let entry = by_type.entry(res.resource_type.clone()).or_insert((0, 0));
        entry.0 += res.quantity; // total quantity
        entry.1 += 1; // reservation count

        // For VirtualMachines, also track by VM type (SKU)
        if res.resource_type == "VirtualMachines" {
            let vm_type_key = format!("VM-{}", res.sku);
            let vm_entry = by_vm_type.entry(vm_type_key).or_insert((0, 0));
            vm_entry.0 += res.quantity;
            vm_entry.1 += 1;
        }

        // Track by expiry month - both units and count
        if let Some(expiry_month) = extract_year_month(&res.expiry_date) {
            let entry = by_expiry_month
                .entry(expiry_month.clone())
                .or_insert((0, 0));
            entry.0 += res.quantity; // units
            entry.1 += 1; // reservation count

            // Track 3-year reservations separately
            if res.term == "P3Y" {
                let entry_3y = by_expiry_month_3y
                    .entry(expiry_month.clone())
                    .or_insert((0, 0));
                entry_3y.0 += res.quantity;
                entry_3y.1 += 1;
            }

            // Also track by resource type and month
            let month = &expiry_month[5..7];
            let type_month_key = (res.resource_type.clone(), month.to_string());
            let type_month_entry = by_type_month.entry(type_month_key).or_insert((0, 0));
            type_month_entry.0 += res.quantity;
            type_month_entry.1 += 1;
        }

        *by_term.entry(res.term.clone()).or_insert(0) += 1;
        total_quantity += res.quantity;
    }

    println!("Summary:");
    println!("  Total Reservations: {}", reservations.len());
    println!("  Total Quantity: {} (Flex units)", total_quantity);

    println!("\n  By Resource Type:");
    let mut by_type_vec: Vec<_> = by_type.iter().collect();
    by_type_vec.sort_by(|a, b| b.1.0.cmp(&a.1.0)); // Sort by quantity descending
    for (rtype, (quantity, count)) in by_type_vec {
        println!("    {}: {} ( {} x Reservations)", rtype, quantity, count);

        // If it's VirtualMachines, show the breakdown by VM type
        if rtype == "VirtualMachines" && !by_vm_type.is_empty() {
            let mut vm_type_vec: Vec<_> = by_vm_type.iter().collect();
            vm_type_vec.sort_by(|a, b| b.1.0.cmp(&a.1.0)); // Sort by quantity descending
            for (vm_type, (vm_quantity, vm_count)) in vm_type_vec {
                println!(
                    "      {}: {} ( {} x Reservations)",
                    vm_type, vm_quantity, vm_count
                );
            }
        }
    }

    print!("\n  By Term:");
    for (term, count) in by_term.iter() {
        let term_str = match term.as_str() {
            "P1Y" => "1 Year",
            "P3Y" => "3 Years",
            _ => term,
        };
        print!("      {}: {}", term_str, count);
    }

    // Aggregate units and counts by month (01-12) across all years
    let mut month_totals: HashMap<String, (u32, u32)> = HashMap::new(); // (units, count)
    for (year_month, (units, count)) in by_expiry_month.iter() {
        let month = &year_month[5..7];
        let entry = month_totals.entry(month.to_string()).or_insert((0, 0));
        entry.0 += units;
        entry.1 += count;
    }

    // Get current month to start from
    let current_month = chrono::Local::now().month();

    // Create a vec with all 12 months starting from current month
    let mut ordered_months = Vec::new();
    for i in 0..12 {
        let month_num = ((current_month - 1 + i) % 12) + 1;
        let month_str = format!("{:02}", month_num);
        let (units, count) = month_totals.get(&month_str).copied().unwrap_or((0, 0));
        ordered_months.push((month_str, units, count));
    }

    // Calculate average units
    let total_units: u32 = ordered_months.iter().map(|(_, units, _)| *units).sum();
    let average_units = total_units as f64 / 12.0;

    // ANSI color codes
    const GREEN: &str = "\x1b[32m";
    const RED: &str = "\x1b[31m";
    const RESET: &str = "\x1b[0m";

    println!(
        "\n  Expiry Distribution by Month from {current_month} {current_year} (Combined across all years): (Average units: {average_units:.2})",
        current_month = format_month(&format!("{:02}", current_month)),
        current_year = chrono::Local::now().year(),
        average_units = average_units
    );

    // Find the longest resource type name for alignment
    let resource_types_for_width: Vec<String> = by_type.keys().cloned().collect();
    let max_len = resource_types_for_width
        .iter()
        .map(|s| s.len())
        .max()
        .unwrap_or(6);
    let label_width = max_len.max(6); // Minimum 6 characters

    // First line: ALL - total units/reservations with color based on units
    print!("{:>width$}|", "ALL", width = label_width);
    for (_, units, count) in &ordered_months {
        let color = if *units < average_units as u32 {
            GREEN
        } else if *units > average_units as u32 {
            RED
        } else {
            RESET
        };
        let display = if *count > 0 {
            format!("{}u/{}r", units, count)
        } else {
            "0".to_string()
        };
        print!("{}{:>7}{} |", color, display, RESET);
    }
    println!();

    // Second line: 3year reservations
    // Aggregate 3-year counts by month
    let mut month_totals_3y: HashMap<String, (u32, u32)> = HashMap::new();
    for (year_month, (units, count)) in by_expiry_month_3y.iter() {
        let month = &year_month[5..7];
        let entry = month_totals_3y.entry(month.to_string()).or_insert((0, 0));
        entry.0 += units;
        entry.1 += count;
    }
    
    // Create ordered months for 3-year
    let mut ordered_months_3y = Vec::new();
    for i in 0..12 {
        let month_num = ((current_month - 1 + i) % 12) + 1;
        let month_str = format!("{:02}", month_num);
        let (units, count) = month_totals_3y.get(&month_str).copied().unwrap_or((0, 0));
        ordered_months_3y.push((month_str, units, count));
    }
    
    // Calculate average for 3-year
    let total_units_3y: u32 = ordered_months_3y.iter().map(|(_, units, _)| *units).sum();
    let average_units_3y = total_units_3y as f64 / 12.0;
    
    print!("{:>width$}|", "3year", width = label_width);
    for (_, units, count) in &ordered_months_3y {
        let color = if *units < average_units_3y as u32 && *units > 0 {
            GREEN
        } else if *units > average_units_3y as u32 {
            RED
        } else {
            RESET
        };
        let display = if *count > 0 {
            format!("{}u/{}r", units, count)
        } else {
            "0".to_string()
        };
        print!("{}{:>7}{} |", color, display, RESET);
    }
    println!();

    // Third line: month names
    print!("{:>width$}|", "", width = label_width);
    for (month, _, _) in &ordered_months {
        print!("{:>7} |", format_month(month));
    }
    println!();

    // Print breakdown by resource type
    let mut resource_types: Vec<String> = by_type.keys().cloned().collect();
    resource_types.sort();

    for resource_type in resource_types {
        // Calculate average for this resource type
        let mut type_units_vec = Vec::new();
        for (month, _, _) in &ordered_months {
            let key = (resource_type.clone(), month.clone());
            let (units, _) = by_type_month.get(&key).copied().unwrap_or((0, 0));
            type_units_vec.push(units);
        }
        let type_total_units: u32 = type_units_vec.iter().sum();
        let type_average_units = type_total_units as f64 / 12.0;
        
        print!("{:>width$}|", resource_type, width = label_width);
        for (month, _, _) in &ordered_months {
            let key = (resource_type.clone(), month.clone());
            let (units, count) = by_type_month.get(&key).copied().unwrap_or((0, 0));

            let color = if units < type_average_units as u32 && units > 0 {
                GREEN
            } else if units > type_average_units as u32 {
                RED
            } else {
                RESET
            };
            
            let display = if count > 0 {
                format!("{}u/{}r", units, count)
            } else {
                "".to_string()
            };
            print!("{}{:>7}{} |", color, display, RESET);
        }
        println!();
    }
    println!("\nTHE END.");
}

/// Extract YYYY-MM from date string (format: YYYY-MM-DD)
fn extract_year_month(date: &str) -> Option<String> {
    if date.len() >= 7 {
        Some(date[..7].to_string())
    } else {
        None
    }
}

/// Convert month number to name
fn format_month(month: &str) -> String {
    match month {
        "01" => "Jan",
        "02" => "Feb",
        "03" => "Mar",
        "04" => "Apr",
        "05" => "May",
        "06" => "Jun",
        "07" => "Jul",
        "08" => "Aug",
        "09" => "Sep",
        "10" => "Oct",
        "11" => "Nov",
        "12" => "Dec",
        _ => month,
    }
    .to_string()
}
