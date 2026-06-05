# Tauri Plugin — Bluetooth Manager

**`@vasakgroup/plugin-bluetooth-manager`** is a [Tauri](https://v2.tauri.app) plugin for Linux that provides full Bluetooth adapter and device management via **BlueZ** (the official Linux Bluetooth stack) over **D-Bus**.

> Built with `zbus` 4 and `zvariant` 4. Requires Linux with BlueZ >= 5.x.

---

## Table of Contents

- [Installation](#installation)
- [Architecture](#architecture)
- [TypeScript API](#typescript-api)
  - [Types](#types)
  - [Commands](#commands)
  - [Helper Functions](#helper-functions)
  - [Events](#events)
- [Rust API](#rust-api)
  - [Commands](#rust-commands)
  - [Structures](#rust-structures)
  - [Error Types](#error-types)
- [Permissions](#permissions)
- [Examples](#examples)
- [Troubleshooting](#troubleshooting)
- [Development](#development)

---

## Installation

### 1. Add the Rust crate to your Tauri app

```toml
# src-tauri/Cargo.toml
[dependencies]
tauri-plugin-bluetooth-manager = "2"
```

### 2. Register the plugin

```rust
// src-tauri/src/lib.rs
fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_bluetooth_manager::init())
        .run(tauri::generate_context!())
        .expect("error running tauri app");
}
```

### 3. Install the JS/TS package

```bash
npm install @vasakgroup/plugin-bluetooth-manager
# or
bun add @vasakgroup/plugin-bluetooth-manager
```

### 4. Configure permissions

```jsonc
// src-tauri/capabilities/default.json
{
  "identifier": "default",
  "windows": ["main"],
  "permissions": [
    "bluetooth-manager:default"
  ]
}
```

The `default` permission grants access to all Bluetooth commands. For fine-grained control, see [Permissions](#permissions).

---

## Architecture

The plugin communicates with **BlueZ** (`org.bluez`) through the **D-Bus system bus**. BlueZ is the standard Linux Bluetooth stack and must be running on the system.

```
┌──────────────────────────────────────────────────┐
│                Tauri App (Frontend)               │
│     TypeScript API ←→ invoke("plugin:...")        │
└──────────────────────┬───────────────────────────┘
                       │
┌──────────────────────▼───────────────────────────┐
│           Tauri Plugin (Rust Backend)             │
│                                                   │
│  ┌─────────────┐  ┌──────────────────────────┐   │
│  │  commands.rs │  │      desktop.rs           │   │
│  │  (Commands)  │  │  (Signal Listener)        │   │
│  └──────┬──────┘  └──────────┬───────────────┘   │
│         │                    │                     │
│  ┌──────▼────────────────────▼───────────────┐   │
│  │          zbus 4 / zvariant 4              │   │
│  │        (D-Bus client library)             │   │
│  └──────────────────┬────────────────────────┘   │
└─────────────────────┼────────────────────────────┘
                      │ D-Bus System Bus
┌─────────────────────▼────────────────────────────┐
│                   BlueZ                           │
│              org.bluez (DBus service)             │
│                                                   │
│  ┌──────────────┐  ┌────────────────────────┐    │
│  │  Adapter1    │  │    Device1             │    │
│  │  (hci0)      │  │    (peripheral)        │    │
│  └──────────────┘  └────────────────────────┘    │
└──────────────────────────────────────────────────┘
```

### D-Bus Interfaces Used

| Interface | Purpose |
|-----------|---------|
| `org.freedesktop.DBus.ObjectManager` | Enumerate adapters and devices (`GetManagedObjects`) |
| `org.freedesktop.DBus.Properties` | Read/write adapter and device properties |
| `org.bluez.Adapter1` | Discover, power, configure adapters |
| `org.bluez.Device1` | Connect, disconnect, pair devices |

### Key Implementation Details

- **Property extraction**: BlueZ returns all properties wrapped in D-Bus variants. The plugin auto-unwraps them using `TryFrom<&Value>` and a `get_prop!` macro for ergonomic access.
- **Real-time updates**: The plugin subscribes to BlueZ signals (`InterfacesAdded`, `InterfacesRemoved`, `PropertiesChanged`) via the D-Bus system bus and emits Tauri events to the frontend.
- **Throttling**: Device property changes are throttled to 500ms to avoid flooding the frontend with rapid updates (e.g., RSSI fluctuations during scanning).
- **Error resilience**: D-Bus errors like `InProgress`, `AlreadyConnected`, `NotConnected`, etc. are handled gracefully instead of propagating as hard errors.

---

## TypeScript API

### Types

```typescript
/** Information about a Bluetooth adapter (dongle, built-in) */
interface AdapterInfo {
  path: string;               // D-Bus object path (e.g. "/org/bluez/hci0")
  address: string;            // MAC address (e.g. "00:11:22:33:44:55")
  name: string;               // Adapter name
  alias: string;              // User-configured alias
  class: number;              // Class of device (Bluetooth class)
  powered: boolean;           // Adapter is powered on
  discoverable: boolean;      // Adapter is discoverable by other devices
  discoverableTimeout: number; // Discoverable timeout in seconds
  pairable: boolean;          // Adapter is pairable
  pairableTimeout: number;    // Pairable timeout in seconds
  discovering: boolean;       // Actively scanning for devices
  uuids: string[];            // Supported UUIDs (GATT services)
  modalias?: string;          // Modalias (e.g. "usb:v1D6Bp0246d0540")
}

/** Information about a Bluetooth device (peripheral) */
interface DeviceInfo {
  path: string;               // D-Bus object path
  address: string;            // MAC address
  name?: string;              // Device name (may be null during discovery)
  alias?: string;             // User-configured alias
  class?: number;             // Class of device
  appearance?: number;        // Appearance (Bluetooth LE)
  icon?: string;              // Icon identifier
  paired: boolean;            // Device is paired
  trusted: boolean;           // Device is trusted (auto-connect)
  blocked: boolean;           // Device is blocked
  legacyPairing: boolean;     // Uses legacy pairing (SSP)
  rssi?: number;              // Signal strength (dBm)
  txPower?: number;           // Transmit power (dBm)
  connected: boolean;         // Device is connected
  uuids: string[];            // Supported UUIDs
  adapter: string;            // D-Bus path of parent adapter
  servicesResolved: boolean;  // All services are resolved
}

/** Event payload for real-time Bluetooth changes */
interface BluetoothChange {
  changeType: string;         // Type of change (see Events)
  data: any;                  // AdapterInfo, DeviceInfo, or path
}
```

### Commands

All commands are async and return Promises. Errors are thrown as exceptions.

| Function | Returns | Description |
|----------|---------|-------------|
| `listAdapters()` | `AdapterInfo[]` | List all Bluetooth adapters |
| `setAdapterPowered(path, powered)` | `void` | Turn adapter on/off |
| `getAdapterState(path)` | `AdapterInfo` | Get adapter current state |
| `listDevices(adapterPath)` | `DeviceInfo[]` | List all devices for an adapter |
| `getDeviceInfo(devicePath)` | `DeviceInfo` | Get detailed device info |
| `listPairedDevices(adapterPath)` | `DeviceInfo[]` | List only paired devices |
| `startScan(adapterPath)` | `void` | Start device discovery (scan) |
| `stopScan(adapterPath)` | `void` | Stop device discovery |
| `connectDevice(devicePath)` | `void` | Connect to a device |
| `disconnectDevice(devicePath)` | `void` | Disconnect from a device |
| `isBluetoothPluginInitialized()` | `boolean` | Check if plugin initialized correctly |

```typescript
import {
  listAdapters,
  setAdapterPowered,
  getAdapterState,
  listDevices,
  getDeviceInfo,
  listPairedDevices,
  startScan,
  stopScan,
  connectDevice,
  disconnectDevice,
  isBluetoothPluginInitialized,
} from '@vasakgroup/plugin-bluetooth-manager';

// List adapters
const adapters = await listAdapters();
console.log(adapters[0].address); // "00:11:22:33:44:55"

// Power on
await setAdapterPowered('/org/bluez/hci0', true);

// Disconnect a device
await disconnectDevice('/org/bluez/hci0/dev_XX_XX_XX_XX_XX_XX');
```

### Helper Functions

The package includes ergonomic wrappers:

```typescript
import {
  isBluetoothAvailable,
  getDefaultAdapter,
  isBluetoothEnabled,
  toggleBluetooth,
  getConnectedDevicesCount,
  getConnectedDevices,
  getAvailableDevices,
  findDeviceByAddress,
  scanForDevices,
} from '@vasakgroup/plugin-bluetooth-manager';

// Quick check
if (await isBluetoothAvailable()) {
  const adapter = await getDefaultAdapter();
  const enabled = await isBluetoothEnabled();

  // Toggle power
  const newState = await toggleBluetooth();

  // Scan for 10 seconds
  const devices = await scanForDevices(adapter!.path, 10000);

  // Find a specific device
  const device = await findDeviceByAddress(adapter!.path, 'XX:XX:XX:XX:XX:XX');
}

// Get connected devices
const connected = await getConnectedDevices('/org/bluez/hci0');
console.log(`${connected.length} device(s) connected`);
```

### Events

The plugin emits real-time events via Tauri's event system. Listen with `@tauri-apps/api/event`:

```typescript
import { listen } from '@tauri-apps/api/event';
import { BluetoothChangeType } from '@vasakgroup/plugin-bluetooth-manager';

await listen('bluetooth-change', (event) => {
  const { changeType, data } = event.payload;

  switch (changeType) {
    case BluetoothChangeType.ADAPTER_ADDED:
      console.log('New adapter:', data);
      break;
    case BluetoothChangeType.ADAPTER_REMOVED:
      console.log('Adapter removed:', data.path);
      break;
    case BluetoothChangeType.ADAPTER_PROPERTY_CHANGED:
      console.log('Adapter property changed:', data);
      break;
    case BluetoothChangeType.DEVICE_ADDED:
      console.log('New device discovered:', data);
      break;
    case BluetoothChangeType.DEVICE_REMOVED:
      console.log('Device removed:', data.path);
      break;
    case BluetoothChangeType.DEVICE_CONNECTED:
      console.log('Device connected:', data);
      break;
    case BluetoothChangeType.DEVICE_DISCONNECTED:
      console.log('Device disconnected:', data);
      break;
    case BluetoothChangeType.DEVICE_PROPERTY_CHANGED:
      console.log('Device property changed:', data);
      break;
    case BluetoothChangeType.ERROR:
      console.error('Bluetooth error:', data.message);
      break;
    case BluetoothChangeType.DBUS_ERROR:
      console.error('D-Bus error:', data.message);
      break;
  }
});
```

#### Event Types

| `changeType` | `data` shape | Triggered when |
|---|---|---|
| `adapter-added` | `AdapterInfo` | New Bluetooth adapter appears |
| `adapter-removed` | `{ path: string }` | Adapter is removed |
| `adapter-property-changed` | `AdapterInfo` | Adapter property changes (power, name, etc.) |
| `device-added` | `DeviceInfo` | New device discovered during scan |
| `device-removed` | `{ path: string }` | Device is removed/unpaired |
| `device-connected` | `DeviceInfo` | Device connects |
| `device-disconnected` | `DeviceInfo` | Device disconnects |
| `device-property-changed` | `DeviceInfo` | Device property changes (RSSI, name, etc.) |
| `error` | `{ message: string }` | Internal plugin error |
| `dbus-error` | `{ message: string }` | D-Bus stream error (fatal, listener stops) |

---

## Rust API

### Commands

`src/commands.rs` — Each function is a `#[tauri::command]`:

| Command | Input | Output | BlueZ Method |
|---------|-------|--------|-------------|
| `list_adapters` | — | `Vec<AdapterInfo>` | `GetManagedObjects` |
| `set_adapter_powered` | `adapter_path`, `powered: bool` | `()` | `Properties.Set` |
| `get_adapter_state` | `adapter_path` | `AdapterInfo` | `Properties.GetAll` |
| `start_scan` | `adapter_path` | `()` | `StartDiscovery` |
| `stop_scan` | `adapter_path` | `()` | `StopDiscovery` |
| `list_devices` | `adapter_path` | `Vec<DeviceInfo>` | `GetManagedObjects` |
| `get_device_info` | `device_path` | `DeviceInfo` | `Properties.GetAll` |
| `list_paired_devices` | `adapter_path` | `Vec<DeviceInfo>` | `GetManagedObjects` |
| `connect_device` | `device_path` | `()` | `Connect` |
| `disconnect_device` | `device_path` | `()` | `Disconnect` |
| `bluetooth_plugin_status` | `State<BluetoothManager>` | `bool` | — |

### Structures

`src/models.rs`:

```rust
#[derive(Serialize, Debug, Clone)]
pub struct AdapterInfo {
    pub path: String,
    pub address: String,
    pub name: String,
    pub alias: String,
    pub class: u32,
    pub powered: bool,
    pub discoverable: bool,
    pub discoverable_timeout: u32,
    pub pairable: bool,
    pub pairable_timeout: u32,
    pub discovering: bool,
    pub uuids: Vec<String>,
    pub modalias: Option<String>,
}

#[derive(Serialize, Debug, Clone)]
pub struct DeviceInfo {
    pub path: String,
    pub address: String,
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
    pub tx_power: Option<i16>,
    pub connected: bool,
    pub uuids: Vec<String>,
    pub adapter: String,
    pub services_resolved: bool,
}
```

### Error Types

`src/error.rs`:

```rust
#[derive(Debug, Error)]
pub enum Error {
    Zbus(#[from] zbus::Error),
    Zvariant(#[from] zbus::zvariant::Error),
    CommandError(String),
    NotFound(String),
}
```

Errors implement `Serialize` (display as string) so they propagate correctly to the frontend.

---

## Permissions

### Default Permission

`"bluetooth-manager:default"` — allows all Bluetooth commands:

```toml
# permissions/default.toml
[default]
permissions = [
  "allow-list_adapters",
  "allow-list_devices",
  "allow-list_paired_devices",
  "allow-set_adapter_powered",
  "allow-start_scan",
  "allow-stop_scan",
  "allow-connect_device",
  "allow-disconnect_device",
  "allow-get_device_info",
  "allow-bluetooth_plugin_status",
]
```

### Individual Commands

You can selectively allow specific commands:

```jsonc
// src-tauri/capabilities/default.json
{
  "permissions": [
    "bluetooth-manager:allow-list_adapters",
    "bluetooth-manager:allow-list_devices",
    "bluetooth-manager:allow-start_scan",
    "bluetooth-manager:allow-stop_scan"
  ]
}
```

---

## Examples

### Minimal: List and scan

```typescript
import { listAdapters, startScan, stopScan, listDevices } from '@vasakgroup/plugin-bluetooth-manager';

const adapters = await listAdapters();
if (adapters.length === 0) throw new Error('No Bluetooth adapter');

const adapter = adapters[0];
await setAdapterPowered(adapter.path, true);

// Scan for 5 seconds
await startScan(adapter.path);
await new Promise(r => setTimeout(r, 5000));
await stopScan(adapter.path);

const devices = await listDevices(adapter.path);
console.log(`Found ${devices.length} device(s):`);
devices.forEach(d => console.log(`  ${d.address} — ${d.name ?? 'Unknown'}`));
```

### Full: Pair and connect flow

```typescript
import { scanForDevices, connectDevice, listPairedDevices } from '@vasakgroup/plugin-bluetooth-manager';
import { listen } from '@tauri-apps/api/event';

// Listen for device discoveries in real time
await listen('bluetooth-change', (event) => {
  if (event.payload.changeType === 'device-added') {
    console.log('Discovered:', event.payload.data.name);
  }
});

// Scan via helper
const devices = await scanForDevices('/org/bluez/hci0', 15000);

// Find and connect to a specific device
const target = devices.find(d => d.name === 'My Headphones');
if (target && !target.connected) {
  await connectDevice(target.path);
}

// Check paired devices
const paired = await listPairedDevices('/org/bluez/hci0');
```

### React: Status indicator

```tsx
import { useEffect, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { getDefaultAdapter } from '@vasakgroup/plugin-bluetooth-manager';

function BluetoothIndicator() {
  const [powered, setPowered] = useState(false);

  useEffect(() => {
    (async () => {
      const adapter = await getDefaultAdapter();
      if (adapter) setPowered(adapter.powered);
    })();

    const unlisten = await listen('bluetooth-change', (e) => {
      if (e.payload.changeType === 'adapter-property-changed') {
        setPowered(e.payload.data.powered);
      }
    });

    return () => { unlisten(); };
  }, []);

  return <div>{powered ? 'Bluetooth ON' : 'Bluetooth OFF'}</div>;
}
```

---

## Troubleshooting

### No adapters found

1. **Check BlueZ is running**:
   ```bash
   systemctl status bluetooth
   sudo systemctl start bluetooth
   ```

2. **Check Bluetooth hardware**:
   ```bash
   hciconfig -a     # Legacy
   bluetoothctl show # Modern
   ```

3. **Check D-Bus access**: The plugin connects to the **system bus**. Ensure your user has permission (usually granted via the `bluetooth` group):
   ```bash
   sudo usermod -aG bluetooth $USER
   ```

4. **Check logs**: The plugin logs to both stdout (terminal) and `~/.logs/vasak/bluetooth.log`. Set the env var for verbose output:
   ```bash
   RUST_LOG=tauri_plugin_bluetooth_manager=trace ./your-app
   ```

### Devices not found during scan

1. Ensure the adapter is **powered on**: `await setAdapterPowered(path, true)`
2. Ensure the adapter is **discoverable**: `bluetoothctl discoverable on`
3. Check `~/.logs/vasak/bluetooth.log` for deserialization errors from BlueZ
4. Some adapters require that you be in the `bluetooth` group

---

## Development

### Project Structure

```
src/
├── lib.rs          # Plugin entry point, Tauri builder, command registration
├── commands.rs     # All #[tauri::command] functions (D-Bus calls to BlueZ)
├── desktop.rs      # Signal listener, initialization, helper extractors
├── error.rs        # Custom error type (thiserror + serde::Serialize)
├── models.rs       # AdapterInfo, DeviceInfo, BluetoothChange structs
└── logging.rs      # Tracing subscriber (stdout + file), OnceLock-safe init

guest-js/
└── index.ts        # TypeScript API, types, helpers, event constants
```

### Build

```bash
# JS/TS bindings
bun install && bun run build

# Rust library
cargo build

# Full Tauri app
cargo tauri dev
```

### Testing

The plugin requires a running D-Bus system bus with BlueZ:

```bash
# Ensure BlueZ is available
bluetoothctl --version

# Run Rust tests
cargo test
```

--- 

## License

GPL-3.0-or-later — Vasak Group
