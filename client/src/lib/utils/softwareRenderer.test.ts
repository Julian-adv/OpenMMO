import { describe, expect, it } from 'vitest'
import { isSoftwareRendererName } from './softwareRenderer'

describe('isSoftwareRendererName', () => {
  it('flags CPU renderers', () => {
    for (const name of [
      'ANGLE (Google, Vulkan 1.3.0 (SwiftShader Device (Subzero) (0x0000C0DE)), SwiftShader driver)',
      'llvmpipe (LLVM 15.0.7, 256 bits)',
      'ANGLE (Microsoft, Microsoft Basic Render Driver Direct3D11 vs_5_0 ps_5_0, D3D11)',
    ]) {
      expect(isSoftwareRendererName(name)).toBe(true)
    }
  })

  it('passes GPUs and hidden names', () => {
    for (const name of [
      'ANGLE (NVIDIA, NVIDIA GeForce RTX 4090 Direct3D11 vs_5_0 ps_5_0, D3D11)',
      'Apple GPU',
      'WebKit WebGL',
      '',
    ]) {
      expect(isSoftwareRendererName(name)).toBe(false)
    }
  })
})
