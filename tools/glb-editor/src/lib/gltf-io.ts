import type { GLTF } from 'three/examples/jsm/loaders/GLTFLoader.js'
import { GLTFLoader } from 'three/examples/jsm/loaders/GLTFLoader.js'

const loader = new GLTFLoader()

export function loadGLTFFromFile(file: File): Promise<GLTF> {
  return new Promise((resolve, reject) => {
    const url = URL.createObjectURL(file)
    loader.load(
      url,
      (gltf) => {
        URL.revokeObjectURL(url)
        resolve(gltf)
      },
      undefined,
      (err) => {
        URL.revokeObjectURL(url)
        reject(err)
      }
    )
  })
}

export function downloadBlob(fileName: string, blob: Blob): void {
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = fileName
  document.body.appendChild(a)
  a.click()
  document.body.removeChild(a)
  URL.revokeObjectURL(url)
}

export function downloadArrayBuffer(fileName: string, arrayBuffer: ArrayBuffer): void {
  downloadBlob(fileName, new Blob([arrayBuffer], { type: 'model/gltf-binary' }))
}
