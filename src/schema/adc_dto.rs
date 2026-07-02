use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ADCDto {
    pub id: String,
    pub scheduler_typ: String,
    pub request_order: String,
    pub num_of_slots: i64,
    pub slot_width: i64,
    pub timeout: i64,
    pub max_optimization_time: i64,
    pub reject_new_reservations_at: i64,
    pub children: Vec<String>,
}
