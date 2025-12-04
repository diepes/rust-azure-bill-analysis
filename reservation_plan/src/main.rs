mod reservation;
mod print_summary;

use anyhow::Result;
use reservation::{fetch_all_reservations, fetch_all_reservations_force_refresh, fetch_reservation_costs};
use print_summary::print_summary;

fn main() -> Result<()> {
    // Load environment variables from .env file
    dotenv::dotenv().ok();
    
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

    // Fetch costs from Cost Management API
    println!("\nFetching reservation costs...");
    let costs = fetch_reservation_costs().unwrap_or_else(|e| {
        eprintln!("Warning: Failed to fetch costs: {}", e);
        std::collections::HashMap::new()
    });

    // Attach costs to reservations (Cost API returns ReservationOrderId, not ReservationId)
    let mut matched_count = 0;
    let mut total_attached_cost = 0.0;
    let mut low_cost_warnings = Vec::new();
    let mut zero_cost_warnings = Vec::new();
    for res in &mut reservations {
        if let Some(cost) = costs.get(&res.reservation_order_id) {
            res.monthly_cost = Some(*cost);
            total_attached_cost += cost;
            matched_count += 1;
            
            // Check for suspiciously low or zero costs - possible billing issue
            if !is_expired(&res.state) {
                if *cost == 0.0 {
                    zero_cost_warnings.push((res.display_name.clone(), res.resource_type.clone(), res.state.clone()));
                } else if *cost < 1.0 {
                    low_cost_warnings.push((res.display_name.clone(), *cost, res.resource_type.clone()));
                }
            }
            
            if matched_count <= 5 {
                println!("  Matched: {} - {} = ${:.2}", &res.reservation_order_id[..8], res.display_name, cost);
            }
        }
    }
    println!("Matched {} out of {} reservations with cost data (Total: ${:.2})", matched_count, reservations.len(), total_attached_cost);
    
    // Warn about reservations with zero costs
    if !zero_cost_warnings.is_empty() {
        println!("\n⚠️  WARNING: Found {} active reservation(s) with ZERO monthly cost:", zero_cost_warnings.len());
        println!("    This may indicate they're not being used or a billing configuration issue:");
        for (name, rtype, state) in &zero_cost_warnings {
            println!("    - {} ({}) [{}]", name, rtype, state);
        }
        println!();
    }
    
    // Warn about reservations with suspiciously low costs
    if !low_cost_warnings.is_empty() {
        println!("⚠️  WARNING: Found {} reservation(s) with unusually low monthly costs (< $1):", low_cost_warnings.len());
        println!("    This may indicate a billing issue or misconfiguration:");
        for (name, cost, rtype) in &low_cost_warnings {
            println!("    - {} ({}) = ${:.2}/month", name, rtype, cost);
        }
        println!();
    }

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
    println!("{}", "=".repeat(180));

    // Print in table format
    println!(
        "{:<10} {:<40} {:<25} {:<5} {:<15} {:<12} {:<12} {:<6} {:<10} {:<5} {:<12}",
        "OrderId",
        "DisplayName",
        "SKU",
        "Qty",
        "Type",
        "Purchase",
        "Expiry",
        "Term",
        "State",
        "Flex",
        "Monthly Cost"
    );
    println!("{}", "-".repeat(180));

    for res in &reservations {
        let order_id_short = &res.reservation_order_id[..8.min(res.reservation_order_id.len())];
        let flex = res.instance_flexibility.as_deref().unwrap_or("N/A");
        let cost_str = match res.monthly_cost {
            Some(cost) => format!("${:.2}", cost),
            None => "N/A".to_string(),
        };
        println!(
            "{:<10} {:<40} {:<25} {:<5} {:<15} {:<12} {:<12} {:<6} {:<10} {:<5} {:<12}",
            order_id_short,
            res.display_name,
            res.sku,
            res.quantity,
            res.resource_type,
            res.purchase_date,
            res.expiry_date,
            res.term,
            res.state,
            flex,
            cost_str
        );
    }
    println!("{}", "=".repeat(180));
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
