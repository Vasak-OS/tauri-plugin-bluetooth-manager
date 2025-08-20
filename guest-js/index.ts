import { invoke } from '@tauri-apps/api/core'

// ============================================================================
// TYPES / INTERFACES
// ============================================================================

export interface PingRequest {
  value?: string;
}

export interface PingResponse {
  value?: string;
}

export interface AdapterInfo {
  path: string;
  address: string; // MAC address
  name: string;
  alias: string;
  class: number; // Class of device
  powered: boolean;
  discoverable: boolean;
  discoverableTimeout: number;
  pairable: boolean;
  pairableTimeout: number;
  discovering: boolean;
  uuids: string[];
  modalias?: string; // Ejemplo: "usb:v1D6Bp0246d0540"
}

export interface DeviceInfo {
  path: string;
  address: string; // MAC address
  name?: string;
  alias?: string;
  class?: number;
  appearance?: number;
  icon?: string;
  paired: boolean;
  trusted: boolean;
  blocked: boolean;
  legacyPairing: boolean;
  rssi?: number;
  txPower?: number; // TxPower
  connected: boolean;
  uuids: string[];
  adapter: string; // ObjectPath del adaptador al que pertenece
  servicesResolved: boolean;
}

export interface BluetoothChange {
  changeType: string;
  data: any;
}

// ============================================================================
// API FUNCTIONS
// ============================================================================

/**
 * Get list of all Bluetooth adapters
 */
export async function listAdapters(): Promise<AdapterInfo[]> {
  return await invoke<AdapterInfo[]>('plugin:bluetooth-manager|list_adapters');
}

/**
 * Set adapter power state (on/off)
 */
export async function setAdapterPowered(adapterPath: string, powered: boolean): Promise<void> {
  return await invoke<void>('plugin:bluetooth-manager|set_adapter_powered', {
    adapterPath,
    powered,
  });
}

/**
 * Get current state of a specific adapter
 */
export async function getAdapterState(adapterPath: string): Promise<AdapterInfo> {
  return await invoke<AdapterInfo>('plugin:bluetooth-manager|get_adapter_state', {
    adapterPath,
  });
}

/**
 * Start device discovery (scan) on an adapter
 */
export async function startScan(adapterPath: string): Promise<void> {
  return await invoke<void>('plugin:bluetooth-manager|start_scan', {
    adapterPath,
  });
}

/**
 * Stop device discovery (scan) on an adapter
 */
export async function stopScan(adapterPath: string): Promise<void> {
  return await invoke<void>('plugin:bluetooth-manager|stop_scan', {
    adapterPath,
  });
}

/**
 * List all devices associated with an adapter
 */
export async function listDevices(adapterPath: string): Promise<DeviceInfo[]> {
  return await invoke<DeviceInfo[]>('plugin:bluetooth-manager|list_devices', {
    adapterPath,
  });
}

/**
 * Get detailed information about a specific device
 */
export async function getDeviceInfo(devicePath: string): Promise<DeviceInfo> {
  return await invoke<DeviceInfo>('plugin:bluetooth-manager|get_device_info', {
    devicePath,
  });
}

/**
 * List only paired devices for an adapter
 */
export async function listPairedDevices(adapterPath: string): Promise<DeviceInfo[]> {
  return await invoke<DeviceInfo[]>('plugin:bluetooth-manager|list_paired_devices', {
    adapterPath,
  });
}

/**
 * Connect to a Bluetooth device
 */
export async function connectDevice(devicePath: string): Promise<void> {
  return await invoke<void>('plugin:bluetooth-manager|connect_device', {
    devicePath,
  });
}

/**
 * Disconnect from a Bluetooth device
 */
export async function disconnectDevice(devicePath: string): Promise<void> {
  return await invoke<void>('plugin:bluetooth-manager|disconnect_device', {
    devicePath,
  });
}

/**
 * Check if the bluetooth plugin was initialized correctly
 */
export async function isBluetoothPluginInitialized(): Promise<boolean> {
  return await invoke<boolean>('plugin:bluetooth-manager|bluetooth_plugin_status');
}

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/**
 * Check if Bluetooth is available (has adapters)
 */
export async function isBluetoothAvailable(): Promise<boolean> {
  try {
    const adapters = await listAdapters();
    return adapters.length > 0;
  } catch (error) {
    console.error('Error checking Bluetooth availability:', error);
    return false;
  }
}

/**
 * Get the first available adapter (most common use case)
 */
export async function getDefaultAdapter(): Promise<AdapterInfo | null> {
  try {
    const adapters = await listAdapters();
    return adapters.length > 0 ? adapters[0] : null;
  } catch (error) {
    console.error('Error getting default adapter:', error);
    return null;
  }
}

/**
 * Check if Bluetooth is currently enabled
 */
export async function isBluetoothEnabled(): Promise<boolean> {
  try {
    const adapter = await getDefaultAdapter();
    return adapter ? adapter.powered : false;
  } catch (error) {
    console.error('Error checking Bluetooth enabled state:', error);
    return false;
  }
}

/**
 * Toggle Bluetooth on/off for the default adapter
 */
export async function toggleBluetooth(): Promise<boolean> {
  try {
    const adapter = await getDefaultAdapter();
    if (!adapter) {
      throw new Error('No Bluetooth adapter found');
    }
    
    const newState = !adapter.powered;
    await setAdapterPowered(adapter.path, newState);
    return newState;
  } catch (error) {
    console.error('Error toggling Bluetooth:', error);
    throw error;
  }
}

/**
 * Get connected devices count for an adapter
 */
export async function getConnectedDevicesCount(adapterPath: string): Promise<number> {
  try {
    const devices = await listDevices(adapterPath);
    return devices.filter(device => device.connected).length;
  } catch (error) {
    console.error('Error getting connected devices count:', error);
    return 0;
  }
}

/**
 * Get all connected devices for an adapter
 */
export async function getConnectedDevices(adapterPath: string): Promise<DeviceInfo[]> {
  try {
    const devices = await listDevices(adapterPath);
    return devices.filter(device => device.connected);
  } catch (error) {
    console.error('Error getting connected devices:', error);
    return [];
  }
}

/**
 * Get available (non-connected) devices for an adapter
 */
export async function getAvailableDevices(adapterPath: string): Promise<DeviceInfo[]> {
  try {
    const devices = await listDevices(adapterPath);
    return devices.filter(device => !device.connected);
  } catch (error) {
    console.error('Error getting available devices:', error);
    return [];
  }
}

/**
 * Find device by address
 */
export async function findDeviceByAddress(adapterPath: string, address: string): Promise<DeviceInfo | null> {
  try {
    const devices = await listDevices(adapterPath);
    return devices.find(device => device.address.toLowerCase() === address.toLowerCase()) || null;
  } catch (error) {
    console.error('Error finding device by address:', error);
    return null;
  }
}

/**
 * Start scanning and return a promise that resolves after a timeout
 */
export async function scanForDevices(adapterPath: string, timeoutMs: number = 10000): Promise<DeviceInfo[]> {
  try {
    await startScan(adapterPath);
    
    // Wait for the specified timeout
    await new Promise(resolve => setTimeout(resolve, timeoutMs));
    
    await stopScan(adapterPath);
    return await listDevices(adapterPath);
  } catch (error) {
    console.error('Error during device scan:', error);
    try {
      await stopScan(adapterPath); // Ensure scan is stopped
    } catch (stopError) {
      console.error('Error stopping scan:', stopError);
    }
    throw error;
  }
}

// ============================================================================
// BLUETOOTH CHANGE EVENT TYPES
// ============================================================================

export const BluetoothChangeType = {
  ADAPTER_ADDED: 'adapter-added',
  ADAPTER_REMOVED: 'adapter-removed',
  ADAPTER_PROPERTY_CHANGED: 'adapter-property-changed',
  DEVICE_ADDED: 'device-added',
  DEVICE_REMOVED: 'device-removed',
  DEVICE_CONNECTED: 'device-connected',
  DEVICE_DISCONNECTED: 'device-disconnected',
  DEVICE_PROPERTY_CHANGED: 'device-property-changed',
  ERROR: 'error',
  DBUS_ERROR: 'dbus-error',
} as const;

export type BluetoothChangeTypeValue = typeof BluetoothChangeType[keyof typeof BluetoothChangeType];