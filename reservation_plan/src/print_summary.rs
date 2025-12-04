use crate::reservation::Reservation;
use std::collections::HashMap;
use chrono::{Datelike, Local, NaiveDate};

pub fn print_summary(reservations: &[Reservation]) {
    let mut by_type: HashMap<String, (u32, u32, f64, Vec<u32>)> = HashMap::new(); // (total_quantity, reservation_count, monthly_cost, remaining_months_list)
    let mut by_vm_type: HashMap<String, (u32, u32, f64, Vec<u32>)> = HashMap::new(); // VM types breakdown with cost and remaining months
    let mut by_term: HashMap<String, u32> = HashMap::new();
    let mut by_expiry_month: HashMap<String, (u32, u32)> = HashMap::new(); // (units, reservation_count)
    let mut by_expiry_month_3y: HashMap<String, (u32, u32)> = HashMap::new(); // 3-year term only
    let mut by_type_month: HashMap<(String, String), (u32, u32)> = HashMap::new(); // (resource_type, month) -> (units, count)
    let mut cost_by_expiry_month: HashMap<String, f64> = HashMap::new(); // total cost by expiry month
    let mut total_quantity = 0;
    let mut total_monthly_cost = 0.0;

    let current_date = Local::now().date_naive();

    for res in reservations {
        let res_cost = res.monthly_cost.unwrap_or(0.0);
        total_monthly_cost += res_cost;

        // Calculate remaining months for this reservation
        let remaining_months = if let Ok(expiry_date) = NaiveDate::parse_from_str(&res.expiry_date, "%Y-%m-%d") {
            if expiry_date > current_date {
                let years_diff = expiry_date.year() - current_date.year();
                let months_diff = expiry_date.month() as i32 - current_date.month() as i32;
                let total_months = years_diff * 12 + months_diff;
                total_months.max(0) as u32
            } else {
                0
            }
        } else {
            0
        };

        // Track by resource type
        let entry = by_type.entry(res.resource_type.clone()).or_insert((0, 0, 0.0, Vec::new()));
        entry.0 += res.quantity; // total quantity
        entry.1 += 1; // reservation count
        entry.2 += res_cost; // monthly cost
        entry.3.push(remaining_months); // remaining months list

        // For VirtualMachines, also track by VM type (SKU)
        if res.resource_type == "VirtualMachines" {
            let vm_type_key = format!("VM-{}", res.sku.to_lowercase());
            let vm_entry = by_vm_type.entry(vm_type_key).or_insert((0, 0, 0.0, Vec::new()));
            vm_entry.0 += res.quantity;
            vm_entry.1 += 1;
            vm_entry.2 += res_cost;
            vm_entry.3.push(remaining_months);
        }

        // Track by expiry month - both units and count
        if let Some(expiry_month) = extract_year_month(&res.expiry_date) {
            let entry = by_expiry_month
                .entry(expiry_month.clone())
                .or_insert((0, 0));
            entry.0 += res.quantity; // units
            entry.1 += 1; // reservation count

            // Track cost by expiry month
            *cost_by_expiry_month.entry(expiry_month.clone()).or_insert(0.0) += res_cost;

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
    println!("  Total Monthly Cost: ${:.2}", total_monthly_cost);

    println!("\n  By Resource Type:");
    let mut by_type_vec: Vec<_> = by_type.iter().collect();
    by_type_vec.sort_by(|a, b| b.1.0.cmp(&a.1.0)); // Sort by quantity descending
    for (rtype, (quantity, count, cost, remaining_months_list)) in by_type_vec {
        let avg_remaining = if !remaining_months_list.is_empty() {
            remaining_months_list.iter().sum::<u32>() as f64 / remaining_months_list.len() as f64
        } else {
            0.0
        };
        println!("    {}: {} ( {} x Reservations) - ${:.2}/month (~{:.0} months remaining)", rtype, quantity, count, cost, avg_remaining);

        // If it's VirtualMachines, show the breakdown by VM type, sorted by quantity
        if rtype == "VirtualMachines" && !by_vm_type.is_empty() {
            let mut vm_type_vec: Vec<_> = by_vm_type.iter().collect();
            vm_type_vec.sort_by(|a, b| b.1.0.cmp(&a.1.0)); // Sort by quantity descending
            for (vm_type, (vm_quantity, vm_count, vm_cost, vm_remaining_months_list)) in vm_type_vec {
                let vm_avg_remaining = if !vm_remaining_months_list.is_empty() {
                    vm_remaining_months_list.iter().sum::<u32>() as f64 / vm_remaining_months_list.len() as f64
                } else {
                    0.0
                };
                println!(
                    "      {}: {} ( {} x Reservations) - ${:.2}/month (~{:.0} months remaining)",
                    vm_type, vm_quantity, vm_count, vm_cost, vm_avg_remaining
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
    let mut month_cost_totals: HashMap<String, f64> = HashMap::new(); // total cost by month
    for (year_month, (units, count)) in by_expiry_month.iter() {
        let month = &year_month[5..7];
        let entry = month_totals.entry(month.to_string()).or_insert((0, 0));
        entry.0 += units;
        entry.1 += count;
        
        // Also aggregate cost by month
        if let Some(cost) = cost_by_expiry_month.get(year_month) {
            *month_cost_totals.entry(month.to_string()).or_insert(0.0) += cost;
        }
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

    // First line: month names
    print!("{:>width$}|", "", width = label_width);
    for (month, _, _) in &ordered_months {
        print!("{:>8} |", format_month(month));
    }
    println!();

    // Second line: ALL - total units/reservations with color based on units
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
            "-".to_string()
        };
        print!("{}{:>8}{} |", color, display, RESET);
    }
    println!();

    // Third line: Cost - total commitment for each month
    // Calculate average cost for color coding
    let total_cost: f64 = month_cost_totals.values().sum();
    let average_cost = total_cost / 12.0;
    
    print!("{:>width$}|", "Commitment/m", width = label_width);
    for (month, _, _) in &ordered_months {
        let cost = month_cost_totals.get(month).copied().unwrap_or(0.0);
        let color = if cost > 0.0 {
            if cost < average_cost - 4000.0 {
                GREEN
            } else if cost > average_cost + 4000.0 {
                RED
            } else {
                RESET
            }
        } else {
            RESET
        };
        let display = if cost > 0.0 {
            format!("${:.0}k", cost / 1000.0)
        } else {
            "-".to_string()
        };
        print!("{}{:>8}{} |", color, display, RESET);
    }
    println!();

    // Fourth line: 3year reservations
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
            "-".to_string()
        };
        print!("{}{:>8}{} |", color, display, RESET);
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
                "-".to_string()
            };
            print!("{}{:>8}{} |", color, display, RESET);
        }
        println!();
    }
    
    // Last line: month names (repeated at bottom)
    print!("{:>width$}|", "", width = label_width);
    for (month, _, _) in &ordered_months {
        print!("{:>8} |", format_month(month));
    }
    println!();
    
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
