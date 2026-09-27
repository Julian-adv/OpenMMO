import catalog from '../../../../data/object_catalog.json'
import type { ObjectDef } from '../stores/editorStore'

export const objectDefs = catalog as ObjectDef[]
export const objectDefsById = new Map(objectDefs.map((def) => [def.id, def]))

export function getObjectDef(id: string | undefined): ObjectDef | undefined {
  return id === undefined ? undefined : objectDefsById.get(id)
}
