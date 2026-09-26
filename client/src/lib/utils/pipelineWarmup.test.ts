import { describe, expect, it, vi } from 'vitest'
import { Group, PerspectiveCamera, Scene } from 'three'
import { warmupPipelines } from './pipelineWarmup'

vi.mock('./frameYield', () => ({ yieldTask: async () => {} }))

function deferred() {
  let resolve!: () => void
  const promise = new Promise<void>((done) => {
    resolve = done
  })
  return { promise, resolve }
}

function context() {
  return {
    renderer: {
      init: vi.fn(async () => {}),
      compileAsync: vi.fn(async () => {}),
    },
    camera: { current: new PerspectiveCamera() },
    scene: new Scene(),
  }
}

describe('model pipeline warmup', () => {
  it('serializes different models and shares duplicate requests', async () => {
    const ctx = context()
    const compiled = deferred()
    ctx.renderer.compileAsync.mockReturnValueOnce(compiled.promise)
    const first = warmupPipelines(ctx, 'guard', new Group())
    const duplicate = warmupPipelines(ctx, 'guard', new Group())
    const second = warmupPipelines(ctx, 'merchant', new Group())
    expect(duplicate).toBe(first)
    await vi.waitFor(() =>
      expect(ctx.renderer.compileAsync).toHaveBeenCalledOnce()
    )
    compiled.resolve()
    await Promise.all([first, second])
    expect(ctx.renderer.compileAsync).toHaveBeenCalledTimes(2)
  })

  it('initializes before collecting detached meshes and restores culling immediately', async () => {
    const ctx = context()
    const initialized = deferred()
    const compiled = deferred()
    const model = new Group()
    ctx.renderer.init.mockReturnValue(initialized.promise)
    ctx.renderer.compileAsync.mockImplementation(() => {
      expect(model.frustumCulled).toBe(false)
      return compiled.promise
    })
    const work = warmupPipelines(ctx, 'guard', model)
    await vi.waitFor(() => expect(ctx.renderer.init).toHaveBeenCalled())
    expect(ctx.renderer.compileAsync).not.toHaveBeenCalled()
    initialized.resolve()
    await vi.waitFor(() => expect(ctx.renderer.compileAsync).toHaveBeenCalled())
    expect(model.frustumCulled).toBe(true)
    compiled.resolve()
    await work
  })

  it('continues preparing models after one compilation fails', async () => {
    const ctx = context()
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {})
    ctx.renderer.compileAsync.mockRejectedValueOnce(new Error('failed'))
    try {
      await Promise.all([
        warmupPipelines(ctx, 'broken', new Group()),
        warmupPipelines(ctx, 'guard', new Group()),
      ])
      expect(ctx.renderer.compileAsync).toHaveBeenCalledTimes(2)
      expect(warn).toHaveBeenCalledOnce()
    } finally {
      warn.mockRestore()
    }
  })
})
