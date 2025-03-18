#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub enum CostType {
    ResourceName,
    ResourceGroup,
    Subscription,
    MeterCategory,
    MeterSubCategory,
    Tag,
    Reservation,
    Region, //Location
}
impl CostType {
    pub fn as_str(&self) -> &str {
        match self {
            CostType::ResourceName => "ResourceName",
            CostType::ResourceGroup => "ResourceGroup",
            CostType::Subscription => "Subscription",
            CostType::MeterCategory => "MeterCategory",
            CostType::MeterSubCategory => "MeterSubCategory",
            CostType::Tag => "Tag",
            CostType::Reservation => "Reservation",
            CostType::Region => "Region",
        }
    }
    // short name 3 char
    pub fn as_short(&self) -> &str {
        match self {
            CostType::ResourceName => "Res",
            CostType::ResourceGroup => "Rg",
            CostType::Subscription => "Sub",
            CostType::MeterCategory => "Meter",
            CostType::MeterSubCategory => "MeterSub",
            CostType::Tag => "Tag",
            CostType::Reservation => "Resrv",
            CostType::Region => "Loc",
        }
    }
}
