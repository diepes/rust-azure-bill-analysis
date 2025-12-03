mod reservation;

use anyhow::Result;
use reservation::{fetch_all_reservations, fetch_all_reservations_force_refresh, Reservation};

fn main() -> Result<()> {
    // Check for --refresh or --force flag
    let args: Vec<String> = std::env::args().collect();
    
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_help();
        return Ok(());
    }
    
    let force_refresh = args.iter().any(|arg| arg == "--refresh" || arg == "--force" || arg == "-f");
    
    if force_refresh {
        println!("Force refresh enabled - bypassing cache");
    }
    
    println!("Fetching all active Azure reservations...");
    
    let reservations = if force_refresh {
        fetch_all_reservations_force_refresh()?
    } else {
        fetch_all_reservations()?
    };
    
    println!("\nFound {} active reservations\n", reservations.len());
    
    // Save to JSON file first
    let json_output = serde_json::to_string_pretty(&reservations)?;
    std::fs::write("all_reservations.json", json_output)?;
    println!("Saved to all_reservations.json\n");
    
    // Print summary statistics
    print_summary(&reservations);
    
    println!("\n\nDetailed Reservations:");
    println!("{}", "=".repeat(160));
    
    // Print in table format
    println!(
        "{:<10} {:<40} {:<25} {:<5} {:<15} {:<12} {:<12} {:<6} {:<10} {:<5}",
        "OrderId", "DisplayName", "SKU", "Qty", "Type", "Purchase", "Expiry", "Term", "State", "Flex"
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
    
    Ok(())
}

fn print_help() {
    println!("Azure Reservation Plan Tool");
    println!("\nUsage:");
    println!("  reservation_plan [OPTIONS]");
    println!("\nOptions:");
    println!("  -h, --help              Show this help message");
    println!("  -f, --force, --refresh  Force refresh from Azure (bypass cache)");
    println!("\nDescription:");
    println!("  Fetches all active Azure reservations and displays summary information.");
    println!("  Results are cached in cache_reservations_YYYYMM.json for the current month.");
    println!("  Use --force to bypass cache and fetch fresh data from Azure.");
}

fn print_summary(reservations: &[Reservation]) {
    use std::collections::HashMap;
    
    let mut by_type: HashMap<String, u32> = HashMap::new();
    let mut by_term: HashMap<String, u32> = HashMap::new();
    let mut total_quantity = 0;
    
    for res in reservations {
        *by_type.entry(res.resource_type.clone()).or_insert(0) += 1;
        *by_term.entry(res.term.clone()).or_insert(0) += 1;
        total_quantity += res.quantity;
    }
    
    println!("Summary:");
    println!("  Total Reservations: {}", reservations.len());
    println!("  Total Quantity: {}", total_quantity);
    
    println!("\n  By Resource Type:");
    for (rtype, count) in by_type.iter() {
        println!("    {}: {}", rtype, count);
    }
    
    println!("\n  By Term:");
    for (term, count) in by_term.iter() {
        let term_str = match term.as_str() {
            "P1Y" => "1 Year",
            "P3Y" => "3 Years",
            _ => term,
        };
        println!("    {}: {}", term_str, count);
    }
}
