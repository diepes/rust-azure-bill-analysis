
#[derive(Debug, PartialEq, Eq, Hash)]
pub enum CostType {
    ResourceName,
    ResourceGroup,
    Subscription,
    MeterCategory,
    Tag,
}
impl CostType {
    pub fn as_str(&self) -> &str {
        match self {
            CostType::ResourceName => "ResourceName",
            CostType::ResourceGroup => "ResourceGroup",
            CostType::Subscription => "Subscription",
            CostType::MeterCategory => "MeterCategory",
            CostType::Tag => "Tag",
        }
    }
    // short name 3 char
    pub fn as_short(&self) -> &str {
        match self {
            CostType::ResourceName => "Res",
            CostType::ResourceGroup => "Rg",
            CostType::Subscription => "Sub",
            CostType::MeterCategory => "Meter",
            CostType::Tag => "Tag",
        }
    }
}
