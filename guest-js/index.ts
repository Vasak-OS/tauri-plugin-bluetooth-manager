import { invoke } from '@tauri-apps/api/core'

export async function listAdapters() {
  return await invoke<{adapters: string[]}>('plugin:bluetooth-manager|list_adapters', {
    payload: {},
  }).then((r) => r.adapters);
}