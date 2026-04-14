import { writable } from 'svelte/store'
import type * as THREE from 'three'

export const localPlayerRightHand = writable<THREE.Bone | null>(null)
