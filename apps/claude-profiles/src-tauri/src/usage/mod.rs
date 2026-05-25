pub(crate) mod credentials;
pub(crate) mod quota;

use serde::Serialize;

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileUsage {
    pub quota: Option<QuotaUsage>,
    pub quota_error: Option<QuotaError>,
    pub fetched_at: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaUsage {
    pub five_hour: Option<Window>,
    pub seven_day: Option<Window>,
    pub seven_day_sonnet: Option<Window>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Window {
    pub utilization: Option<f32>,
    pub resets_at: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QuotaError {
    NoCredentials,
    Unauthorized,
    Network,
    Unknown,
}
