/** Renderer names browsers report when they draw on the CPU: Chrome/Edge with
 *  hardware acceleration off (SwiftShader), Mesa (llvmpipe/softpipe), and
 *  Windows without a GPU driver (Microsoft Basic Render Driver). */
const SOFTWARE_RENDERERS = /swiftshader|llvmpipe|softpipe|basic render/i

export function isSoftwareRendererName(name: string): boolean {
  return SOFTWARE_RENDERERS.test(name)
}

let detected: boolean | undefined

/** Whether WebGL in this browser runs without the GPU, read once from a
 *  throwaway context. False when the browser hides the renderer name. */
export function detectSoftwareRenderer(): boolean {
  return (detected ??= readSoftwareRenderer())
}

function readSoftwareRenderer(): boolean {
  if (typeof document === 'undefined') return false
  const gl = document.createElement('canvas').getContext('webgl2')
  if (!gl) return false
  try {
    const info = gl.getExtension('WEBGL_debug_renderer_info')
    return isSoftwareRendererName(
      String(gl.getParameter(info ? info.UNMASKED_RENDERER_WEBGL : gl.RENDERER))
    )
  } catch {
    return false
  } finally {
    gl.getExtension('WEBGL_lose_context')?.loseContext()
  }
}
