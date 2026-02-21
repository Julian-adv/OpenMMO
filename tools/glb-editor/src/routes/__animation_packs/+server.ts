import { json } from '@sveltejs/kit'
import type { RequestHandler } from './$types'
import { scanAnimationPackFiles } from '$lib/server/animation-pack-files'

export const GET: RequestHandler = async () => {
  try {
    const packs = await scanAnimationPackFiles()
    return json({ packs })
  } catch (error) {
    return json({ error: String(error) }, { status: 500 })
  }
}
