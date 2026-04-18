import { describe, it, expect } from 'vitest'
import * as THREE from 'three'
import {
  makeSplatStandardMaterial,
  padTileScales,
} from './makeSplatStandardMaterial'
import { MAX_PALETTE } from '../terrain/splat-encoding'

function makeSplatTexture(): THREE.DataTexture {
  const tex = new THREE.DataTexture(
    new Uint8Array(64 * 64 * 4),
    64,
    64,
    THREE.RGBAFormat,
    THREE.UnsignedByteType
  )
  tex.wrapS = tex.wrapT = THREE.ClampToEdgeWrapping
  tex.minFilter = THREE.NearestFilter
  tex.magFilter = THREE.NearestFilter
  return tex
}

function makeAtlasTexture(): THREE.DataTexture {
  return new THREE.DataTexture(
    new Uint8Array(4 * 4 * 4),
    4,
    4,
    THREE.RGBAFormat,
    THREE.UnsignedByteType
  )
}

describe('padTileScales', () => {
  it('pads short array to MAX_PALETTE with 1.0', () => {
    const out = padTileScales([2, 3])
    expect(out.length).toBe(MAX_PALETTE)
    expect(out[0]).toBe(2)
    expect(out[1]).toBe(3)
    expect(out[2]).toBe(1)
    expect(out[MAX_PALETTE - 1]).toBe(1)
  })

  it('truncates oversized array to MAX_PALETTE', () => {
    const tooMany = new Array(MAX_PALETTE + 5).fill(7)
    const out = padTileScales(tooMany)
    expect(out.length).toBe(MAX_PALETTE)
    expect(out.every((v) => v === 7)).toBe(true)
  })
})

describe('makeSplatStandardMaterial', () => {
  // Regression: shader samples 1 texel beyond tile edges. The splatmap MUST
  // remain ClampToEdgeWrapping (set by terrainSplatManager.createTexture);
  // RepeatWrapping pulls opposite-edge cells (sand bands at boundaries).
  it('does not mutate splatMap wrap mode', () => {
    const splatMap = makeSplatTexture()
    splatMap.wrapS = splatMap.wrapT = THREE.ClampToEdgeWrapping
    makeSplatStandardMaterial({
      atlas: {
        diffuseAtlas: makeAtlasTexture(),
        normalAtlas: null,
        ormAtlas: null,
      },
      tileScales: [1],
      splatMap,
    })
    expect(splatMap.wrapS).toBe(THREE.ClampToEdgeWrapping)
    expect(splatMap.wrapT).toBe(THREE.ClampToEdgeWrapping)
  })

  it('forces splatMap to NearestFilter (integer indices must not interpolate)', () => {
    const splatMap = makeSplatTexture()
    splatMap.minFilter = THREE.LinearFilter
    splatMap.magFilter = THREE.LinearFilter
    makeSplatStandardMaterial({
      atlas: {
        diffuseAtlas: makeAtlasTexture(),
        normalAtlas: null,
        ormAtlas: null,
      },
      tileScales: [1],
      splatMap,
    })
    expect(splatMap.minFilter).toBe(THREE.NearestFilter)
    expect(splatMap.magFilter).toBe(THREE.NearestFilter)
    expect(splatMap.generateMipmaps).toBe(false)
    expect(splatMap.anisotropy).toBe(1)
  })

  it('exposes splatMap and atlas textures via userData.uniforms', () => {
    const splatMap = makeSplatTexture()
    const diffuseAtlas = makeAtlasTexture()
    const mat = makeSplatStandardMaterial({
      atlas: { diffuseAtlas, normalAtlas: null, ormAtlas: null },
      tileScales: [2, 4],
      splatMap,
    })
    const u = mat.userData.uniforms
    expect(u.splatMap.value).toBe(splatMap)
    expect(u.diffuseAtlas.value).toBe(diffuseAtlas)
    expect(u.uTileScales.array.length).toBe(MAX_PALETTE)
    expect(u.uTileScales.array[0]).toBe(2)
    expect(u.uTileScales.array[1]).toBe(4)
  })
})
