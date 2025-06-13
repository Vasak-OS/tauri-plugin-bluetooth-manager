use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PingRequest {
  pub value: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PingResponse {
  pub value: Option<String>,
}

#[derive(Serialize, Debug, Clone)]
pub struct AdapterInfo {
    pub path: String,
    pub address: String, // MAC address
    pub name: String,
    pub alias: String,
    pub class: u32, // Class of device
    pub powered: bool,
    pub discoverable: bool,
    pub discoverable_timeout: u32,
    pub pairable: bool,
    pub pairable_timeout: u32,
    pub discovering: bool,
    pub uuids: Vec<String>,
    pub modalias: Option<String>, // Ejemplo: "usb:v1D6Bp0246d0540"
}

#[derive(Serialize, Debug, Clone)]
pub struct DeviceInfo {
    pub path: String,
    pub address: String, // MAC address
    pub name: Option<String>,
    pub alias: Option<String>,
    pub class: Option<u32>,
    pub appearance: Option<u16>,
    pub icon: Option<String>,
    pub paired: bool,
    pub trusted: bool,
    pub blocked: bool,
    pub legacy_pairing: bool,
    pub rssi: Option<i16>,
    pub tx_power: Option<i16>, // TxPower
    pub connected: bool,
    pub uuids: Vec<String>,
    pub adapter: String, // ObjectPath del adaptador al que pertenece
    pub services_resolved: bool,
    // Podríamos añadir `manufacturer_data: Option<HashMap<u16, Vec<u8>>>`
    // y `service_data: Option<HashMap<String, Vec<u8>>>` si es necesario.
}
