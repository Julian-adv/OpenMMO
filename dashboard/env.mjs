import { resolve } from 'node:path'

/**
 * @param {typeof import('vite').loadEnv} loadEnv
 * @param {string} mode
 * @param {string} directory
 */
export function loadDashboardEnv(loadEnv, mode, directory) {
  const env = loadEnv(mode, directory, '')
  env.VITE_GOOGLE_CLIENT_ID = env.VITE_GOOGLE_CLIENT_ID?.trim()
    || loadEnv(mode, resolve(directory, '../client'), 'VITE_GOOGLE_CLIENT_ID').VITE_GOOGLE_CLIENT_ID?.trim()
    || ''
  return env
}
