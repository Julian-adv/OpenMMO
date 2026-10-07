import defaultFace from '../../assets/character-appearance/face-default.png'
import ruggedFace from '../../assets/character-appearance/face-rugged.png'
import rangerFace from '../../assets/character-appearance/face-ranger.png'
import cropHair from '../../assets/character-appearance/hair-crop.png'
import wavyHair from '../../assets/character-appearance/hair-wavy-bone.png'
import rangerHair from '../../assets/character-appearance/hair-ranger.png'

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
