import { WebGPURenderer } from 'three/webgpu'

export function createWebGPURenderer(canvas: HTMLCanvasElement) {
  return new WebGPURenderer({ canvas, antialias: true })
}
