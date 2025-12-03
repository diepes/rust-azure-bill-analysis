use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ReservationRecommendation {
    pub id: String,
    pub name: String,
    pub properties: RecommendationProperties,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecommendationProperties {
    pub look_back_period: String,          // "Last7Days", "Last30Days", "Last60Days"
    pub instance_flexibility_ratio: f64,
    pub instance_flexibility_group: String,
    pub normalized_size: String,
    pub recommended_quantity: f64,
    pub resource_type: String,             // "VirtualMachines"
    pub sku_name: String,                  // "Standard_D2s_v3"
    pub term: String,                      // "P1Y", "P3Y"
    pub cost_with_no_reservation: f64,
    pub recommended_total_cost: f64,
    pub net_savings: f64,
    pub scope: String,                     // "Shared", "Single"
}

pub fn fetch_reservation_recommendations() -> Result<Vec<ReservationRecommendation>> {
    // Get subscription ID
    let subscription_id = get_subscription_id()?;
    
    // Get access token
    let token = get_access_token()?;
    
    // Call REST API
    let url = format!(
        "https://management.azure.com/subscriptions/{}/providers/Microsoft.Consumption/reservationRecommendations?api-version=2023-05-01",
        subscription_id
    );
    
    let output = Command::new("curl")
        .args(&[
            "-X", "GET",
            &url,
            "-H", &format!("Authorization: Bearer {}", token),
            "-H", "Content-Type: application/json",
        ])
        .output()
        .context("Failed to call Azure REST API")?;
    
    let response: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let recommendations = response["value"].as_array()
        .context("No recommendations found")?
        .iter()
        .filter_map(|v| serde_json::from_value(v.clone()).ok())
        .collect();
    
    Ok(recommendations)
}

fn get_subscription_id() -> Result<String> {
    let output = Command::new("az")
        .args(&["account", "show", "--query", "id", "-o", "tsv"])
        .output()?;
    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

fn get_access_token() -> Result<String> {
    let output = Command::new("az")
        .args(&["account", "get-access-token", "--query", "accessToken", "-o", "tsv"])
        .output()?;
    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}