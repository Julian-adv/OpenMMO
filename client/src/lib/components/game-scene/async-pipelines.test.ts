import { afterEach, describe, expect, it, vi } from 'vitest'
import { Mesh, MeshBasicMaterial, PerspectiveCamera, Scene } from 'three'
import type { WebGPURenderer } from 'three/webgpu'
import { installAsyncPipelines } from './async-pipelines'

afterEach(() => vi.restoreAllMocks())

async function setup() {
  let time = 0
  vi.spyOn(performance, 'now').mockImplementation(() => time)
  const scene = new Scene()
  const states = new Map<object, object>()
  const originalDraw = vi.fn((object: object) => {
    time += 8
    states.set(object, {})
  })
  const updateForRender = vi.fn()
  const renderer = {
    init: async () => {},
    _renderObjectDirect: originalDraw,
    _currentRenderContext: {},
    _objects: {
      getChainMap: () => ({ get: ([object]: object[]) => states.get(object) }),
    },
    _pipelines: { updateForRender, getForRender: vi.fn() },
  }
  const hook = installAsyncPipelines(
    renderer as unknown as WebGPURenderer,
    scene
  )
  await Promise.resolve()
  const draw = (object: Mesh, target = scene) => {
    const fn = renderer._renderObjectDirect as (...args: unknown[]) => void
    fn.call(
      renderer,
      object,
      new MeshBasicMaterial(),
      target,
      new PerspectiveCamera(),
      {},
      null,
      null
    )
  }
  return { renderer, hook, draw, states, originalDraw, updateForRender }
}

describe('streamed first draw preparation', () => {
  it('defers cold objects without registering them and keeps existing objects visible', async () => {
    const { hook, draw, states, originalDraw } = await setup()
    const first = new Mesh()
    const second = new Mesh()
    draw(first)
    draw(second)
    expect(states.has(second)).toBe(false)
    draw(first)
    expect(originalDraw).toHaveBeenCalledTimes(2)
    expect(hook.pendingCount()).toBe(1)
    hook.beginFrame()
    draw(second)
    expect(states.has(second)).toBe(true)
    expect(hook.pendingCount()).toBe(0)
  })

  it('reserves preparation time before entering nested shadow passes', async () => {
    const { draw, originalDraw, hook } = await setup()
    originalDraw.mockImplementationOnce(() => draw(new Mesh()))
    draw(new Mesh())
    expect(originalDraw).toHaveBeenCalledOnce()
    expect(hook.pendingCount()).toBe(1)
  })

  it('preserves one-shot bakes and restores hooks on disposal', async () => {
    const { renderer, hook, draw, originalDraw, updateForRender } =
      await setup()
    draw(new Mesh())
    draw(new Mesh(), new Scene())
    expect(originalDraw).toHaveBeenCalledTimes(2)
    hook.dispose()
    expect(renderer._renderObjectDirect).toBe(originalDraw)
    expect(renderer._pipelines.updateForRender).toBe(updateForRender)
  })
})
