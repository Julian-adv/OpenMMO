import sharp from 'sharp'

const dir = 'assets/modular_human_female_01/hair/tripo_warrior_v1/texture_v1'
async function patch(input, path) {
  const mask = Buffer.from(`<svg width="1024" height="1024" xmlns="http://www.w3.org/2000/svg"><defs><filter id="f"><feGaussianBlur stdDeviation="3"/></filter></defs><path d="${path}" fill="white" filter="url(#f)"/></svg>`)
  return sharp(input).resize(1024, 1024, { fit: 'fill' }).ensureAlpha()
    .composite([{ input: mask, blend: 'dest-in' }]).png().toBuffer()
}
const gold = await patch(`${dir}/basecolor-imagegen-v2.png`, 'M 283 15 L 352 15 L 352 68 L 283 68 Z')
const otherSide = await patch(`${dir}/basecolor-imagegen-v2.png`, 'M 175 211 L 286 211 L 295 321 L 175 321 Z')
const generated = await sharp(`${dir}/basecolor-imagegen-v2.png`).resize(1024, 1024).png().toBuffer()
const strand = await sharp(generated).extract({ left: 365, top: 260, width: 60, height: 80 }).resize(75, 85).png().toBuffer()
const strandAtlas = await sharp({ create: { width: 1024, height: 1024, channels: 4, background: '#00000000' } })
  .composite([{ input: strand, left: 797, top: 110 }]).png().toBuffer()
const triangle = await patch(strandAtlas, 'M 806 120 L 853 128 L 853 184 L 815 184 Z')
const leftStrands = await sharp({ create: { width: 1024, height: 1024, channels: 4, background: '#00000000' } })
  .composite([{ input: strand, left: 330, top: 105 }, { input: strand, left: 925, top: 10 }]).png().toBuffer()
const leftTriangles = await patch(leftStrands, 'M 345 132 L 392 128 L 392 176 L 345 176 Z M 935 20 L 986 20 L 988 65 L 935 65 Z')
const overlays = [gold, otherSide, triangle, leftTriangles].map(input => ({ input, blend: 'over' }))
await sharp(`${dir}/basecolor-repaired.png`).composite(overlays)
  .png().toFile(`${dir}/basecolor-repaired-v2.png`)

const normalMask = Buffer.from(`<svg width="1024" height="1024" xmlns="http://www.w3.org/2000/svg"><defs><filter id="f"><feGaussianBlur stdDeviation="4"/></filter></defs><path d="M 327 250 L 376 250 L 382 290 L 327 290 Z M 202 252 L 245 252 L 245 278 L 202 278 Z" fill="white" filter="url(#f)"/></svg>`)
const flatNormal = await sharp({ create: { width: 1024, height: 1024, channels: 4, background: '#8080ff' } })
  .composite([{ input: normalMask, blend: 'dest-in' }]).png().toBuffer()
await sharp(`${dir}/normal-original.jpg`).composite([{ input: flatNormal, blend: 'over' }])
  .png().toFile(`${dir}/normal-repaired-v2.png`)
