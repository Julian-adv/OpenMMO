import sharp from 'sharp'

const dir = 'assets/modular_human_female_01/hair/tripo_warrior_v1/texture_v1'
const mask = Buffer.from(`<svg width="1024" height="1024" xmlns="http://www.w3.org/2000/svg">
  <defs><filter id="feather"><feGaussianBlur stdDeviation="10"/></filter></defs>
  <path d="M 322 229 L 476 223 L 503 269 L 510 338 L 493 386 L 360 385 L 326 349 Z" fill="white" filter="url(#feather)"/>
</svg>`)
const generated = await sharp(`${dir}/basecolor-imagegen.png`)
  .resize(1024, 1024, { fit: 'fill' }).ensureAlpha()
  .composite([{ input: mask, blend: 'dest-in' }]).png().toBuffer()
await sharp(`${dir}/basecolor-original.jpg`)
  .composite([{ input: generated, blend: 'over' }])
  .png().toFile(`${dir}/basecolor-repaired.png`)
