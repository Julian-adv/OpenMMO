import * as THREE from 'three'
import {
  MONSTER_SKILL_DEFS,
  type MonsterSkillDefinition,
} from '../data/monsterSkills'
import { createBarkanRuneSlashEffect } from './barkan-rune-slash'
import { BarkanFireFan } from './barkan-fire-fan'
import { EarthRoarEffect, parseFootprint } from './earth-roar'
import { FormationBreakEffect } from './formation-break'
import { EarthquakeEffect } from './earthquake'
import { preloadMonsterSound } from '../managers/sfxManager'

export interface MonsterSkillCast {
  rotation: number
  range: number
  halfAngle: number
}

export interface MonsterSkillEffects {
  /** Aim the effects at a cast standing at `origin`. */
  start(origin: THREE.Vector3, cast: MonsterSkillCast): void
  /** `time` is the skill clip time; `active` is false between casts. */
  update(
    time: number,
    active: boolean,
    deltaTime: number,
    camera: THREE.Camera | undefined
  ): void
  dispose(): void
}

/** For a skill whose ground marks are the whole effect. */
const NO_EFFECTS: MonsterSkillEffects = {
  start() {},
  update() {},
  dispose() {},
}

// Let the last flames expire after a cast.
const SETTLE_SECONDS = 1
/** Telegraphed monster skills by the server's `MonsterSkillId`. */
export const MONSTER_SKILLS: Record<
  string,
  MonsterSkillDefinition & {
    createEffects(
      scene: THREE.Object3D,
      model: THREE.Object3D,
      weapon: THREE.Object3D | undefined,
      clip: THREE.AnimationClip,
      cast: MonsterSkillCast,
      /** monsters.csv `footprint`, `halfWidth|front|back`. */
      footprint: string | undefined
    ): MonsterSkillEffects
  }
> = {
  rune_slash: {
    ...MONSTER_SKILL_DEFS.rune_slash,
    createEffects(scene, model, weapon, clip, cast) {
      const blade = createBarkanRuneSlashEffect(model, weapon!, clip, {
        light: false,
        bounds: false,
      })
      const fan = new BarkanFireFan(cast.range, cast.halfAngle)
      scene.add(fan.group, fan.particleGroup)
      let idle = 0
      return {
        start(origin, next) {
          fan.group.position.copy(origin)
          fan.group.rotation.y = next.rotation
        },
        update(time, active, deltaTime, camera) {
          idle = active ? 0 : idle + deltaTime
          fan.particleGroup.visible = idle <= SETTLE_SECONDS
          if (!fan.particleGroup.visible) return
          blade.update(time, active)
          fan.update(time, active, deltaTime, camera)
        },
        dispose() {
          blade.dispose()
          fan.dispose()
        },
      }
    },
  },
  earth_roar: {
    ...MONSTER_SKILL_DEFS.earth_roar,
    createEffects(scene, _model, _weapon, _clip, cast, footprint) {
      const roar = new EarthRoarEffect(cast.range, parseFootprint(footprint))
      scene.add(roar.group)
      return {
        start(origin) {
          roar.start(origin)
        },
        update(_time, _active, deltaTime, camera) {
          roar.update(deltaTime, camera)
        },
        dispose() {
          roar.dispose()
        },
      }
    },
  },
  formation_break: {
    ...MONSTER_SKILL_DEFS.formation_break,
    createEffects(scene, _model, _weapon, _clip, cast, footprint) {
      const smash = new FormationBreakEffect(
        cast.range,
        cast.halfAngle,
        parseFootprint(footprint)
      )
      scene.add(smash.group)
      return {
        start(origin, next) {
          smash.start(origin, next.rotation)
        },
        update(_time, _active, deltaTime, camera) {
          smash.update(deltaTime, camera)
        },
        dispose() {
          smash.dispose()
        },
      }
    },
  },
  earthquake: {
    ...MONSTER_SKILL_DEFS.earthquake,
    createEffects(scene, _model, _weapon, _clip, _cast, footprint) {
      const quake = new EarthquakeEffect(parseFootprint(footprint))
      scene.add(quake.group)
      return {
        start(origin) {
          quake.start(origin)
        },
        update(_time, _active, deltaTime, camera) {
          quake.update(deltaTime, camera)
        },
        dispose() {
          quake.dispose()
        },
      }
    },
  },
  crystal_eruption: {
    ...MONSTER_SKILL_DEFS.crystal_eruption,
    createEffects: () => NO_EFFECTS,
  },
}

export function preloadMonsterSkillSounds() {
  for (const { sound, marks } of Object.values(MONSTER_SKILLS)) {
    if (sound) preloadMonsterSound(sound.url)
    if (marks?.sound) preloadMonsterSound(marks.sound)
  }
}
