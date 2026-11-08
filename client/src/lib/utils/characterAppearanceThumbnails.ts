import defaultFace from '../../assets/character-appearance/face-default.png'
import ruggedFace from '../../assets/character-appearance/face-rugged.png'
import rangerFace from '../../assets/character-appearance/face-ranger.png'
import cropHair from '../../assets/character-appearance/hair-crop.png'
import wavyHair from '../../assets/character-appearance/hair-wavy-bone.png'
import rangerHair from '../../assets/character-appearance/hair-ranger.png'
import femaleBob from '../../assets/character-appearance/female-hair-bob.png'
import femaleLong from '../../assets/character-appearance/female-hair-long.png'
import femalePonytail from '../../assets/character-appearance/female-hair-ponytail.png'
import femaleDefaultFace from '../../assets/character-appearance/female-face-default.png'
import femaleCustomFace from '../../assets/character-appearance/female-face-custom.png'

export const FEMALE_FACE_OPTIONS = [
  {
    value: 'default',
    label: 'characterCreate.faceDefault',
    thumbnail: femaleDefaultFace,
  },
  {
    value: 'custom',
    label: 'characterCreate.faceCustom',
    thumbnail: femaleCustomFace,
  },
] as const

export const FACE_OPTIONS = [
  {
    value: 'default',
    label: 'characterCreate.faceDefault',
    thumbnail: defaultFace,
  },
  {
    value: 'rugged',
    label: 'characterCreate.faceRugged',
    thumbnail: ruggedFace,
  },
  {
    value: 'ranger',
    label: 'characterCreate.faceRanger',
    thumbnail: rangerFace,
  },
] as const

export const HAIR_OPTIONS = [
  { value: 'crop', label: 'characterCreate.hairCrop', thumbnail: cropHair },
  {
    value: 'wavy_bone',
    label: 'characterCreate.hairWavyBone',
    thumbnail: wavyHair,
  },
  {
    value: 'ranger',
    label: 'characterCreate.hairRanger',
    thumbnail: rangerHair,
  },
  { value: 'none', label: 'characterCreate.hairNone', thumbnail: defaultFace },
] as const

export const FEMALE_HAIR_OPTIONS = [
  { value: 'bob', label: 'characterCreate.hairBob', thumbnail: femaleBob },
  { value: 'long', label: 'characterCreate.hairLong', thumbnail: femaleLong },
  {
    value: 'ponytail',
    label: 'characterCreate.hairPonytail',
    thumbnail: femalePonytail,
  },
] as const
