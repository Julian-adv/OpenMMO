import assert from 'node:assert/strict'
import { existsSync, mkdirSync, readFileSync, renameSync, writeFileSync } from 'node:fs'
import { NodeIO, PropertyType, VertexLayout } from '@gltf-transform/core'
import { ALL_EXTENSIONS, EXTMeshoptCompression, EXTTextureWebP } from '@gltf-transform/extensions'
import { dedup, prune } from '@gltf-transform/functions'
import { MeshoptDecoder, MeshoptEncoder } from 'meshoptimizer'
import sharp from 'sharp'
import * as THREE from '../client/node_modules/three/build/three.module.js'
import { GLTFExporter } from '../client/node_modules/three/examples/jsm/exporters/GLTFExporter.js'
import { headlessThree, hash } from './lib/headless-three.mjs'
import { dependencies, output as previewOutput } from './lib/female-preview.mjs'
import { ensureFemalePreview } from './lib/ensure-female-preview.mjs'
import { loadHelmetHairBrush } from './lib/helmet-hair-brush.mjs'

const output = 'client/public/models/characters/modular_female'
// Parts the client loads on first equip: packed separately with 1024 px colour textures.
const separatePart = name => /^(caveman|priest|ranger|bard)_/.test(name)
// Fur panels share the waist rig and colliders.
function bakeSkirtRobe(doc) {
  const panels = doc.getRoot().listNodes().filter(node => node.getExtras().pelt_physics)
  const configs = panels.map(node => node.getExtras().pelt_physics)
  const front = Math.max(...configs.map(c => c.pivot[2]))
  const back = Math.min(...configs.map(c => c.pivot[2]))
  const colliders = new Map(configs.flatMap(c => c.colliders).map(c => [JSON.stringify(c), c]))
  const shared = {
    waist_height_m: Math.max(...configs.map(c => c.pivot[1])),
    center_z_m: (front + back) / 2,
    waist_radii_m: [Math.max(...configs.map(c => Math.abs(c.pivot[0]))), (front - back) / 2],
    colliders: [...colliders.values()],
    lift: 0.7,
    follow: 1,
    max_angle: 1.35,
  }
  for (const [i, node] of panels.entries()) {
    const c = configs[i]
    node.setExtras({ ...node.getExtras(), robe_physics: {
      ...shared,
      hem_height_m: c.pivot[1] - c.length,
      driven_below_m: c.pivot[1] - (c.attachment_depth ?? 0),
      fixed_vertices: c.attachment_vertices,
      // The front flap tilts as one piece; blending into side columns widened it while running.
      ...(node.getExtras().panel === 'front' && { columns_deg: [0, 0] }),
    } })
  }
}
const sources = {
  base: 'assets/modular_human_female_01/rig_v1/female-base.glb',
  hair: 'assets/modular_human_female_01/hair/tripo_knight_bob_v1/fitted_v1/hair_female_knight_bob.glb',
  top: 'assets/items/female_knight_top/tripo_v1/fitted_v3/top_female_knight.glb',
  bottom: 'assets/items/female_knight_bottom/tripo_v1/fitted_v42/bottom_female_knight.glb',
  boots: 'assets/items/female_knight_boots/fitted_v5/boots_female_knight.glb',
  gloves: 'assets/items/female_knight_gloves/tripo_v1/fitted_v3/gloves_female_knight.glb',
  helmet: 'assets/items/female_knight_helmet/tripo_v1/fitted_v1/helmet_female_knight.glb',
  barbarian_hair: 'assets/items/female_barbarian_helmet/tripo_v2/fitted_v1/hair_bob_barbarian_helmet.glb',
  barbarian_top: 'assets/items/female_barbarian_top/tripo_v1/fitted_v1/top_female_barbarian.glb',
  barbarian_bottom: 'assets/items/female_barbarian_bottom/tripo_v1/fitted_v1/bottom_female_barbarian.glb',
  barbarian_boots: 'assets/items/female_barbarian_boots/tripo_v1/fitted_v3/boots_female_barbarian.glb',
  barbarian_gloves: 'assets/items/female_barbarian_bracers/tripo_v1/fitted_v1/gloves_female_barbarian.glb',
  barbarian_helmet: 'assets/items/female_barbarian_helmet/tripo_v2/fitted_v1/helmet_female_barbarian.glb',
  rogue_top: 'assets/items/female_rogue_top/tripo_v1/fitted_v6/top_female_rogue.glb',
  rogue_bottom: 'assets/items/female_rogue_skirt/tripo_v1/fitted_v6/bottom_female_rogue.glb',
  rogue_boots: 'assets/items/female_rogue_boots/tripo_v1/fitted_v5/boots_female_rogue.glb',
  rogue_gloves: 'assets/items/female_rogue_gloves/tripo_v1/fitted_v1/gloves_female_rogue.glb',
  rogue_garters: 'assets/items/female_rogue_garters/fitted_v1/garters_female_rogue.glb',
  caveman_top: 'assets/items/female_caveman_top/tripo_v1/fitted_v7/top_female_caveman.glb',
  caveman_bottom: 'assets/items/female_caveman_bottom/tripo_v1/fitted_v9/bottom_female_caveman.glb',
  caveman_boots: 'assets/items/female_caveman_boots/tripo_v1/fitted_v4/boots_female_caveman.glb',
  caveman_gloves: 'assets/items/female_caveman_bracers/tripo_v1/fitted_v1/gloves_female_caveman.glb',
  ...Object.fromEntries(['top', 'bottom', 'boots', 'gloves', 'helmet'].map(slot =>
    [`priest_${slot}`, `assets/items/female_priest_outfit/${slot}/fitted_v1/${slot}_female_priest.glb`])),
  ...Object.fromEntries(['top', 'bottom', 'boots', 'gloves', 'hair'].map(slot =>
    [`ranger_${slot}`, `assets/items/female_ranger_outfit/${slot}/fitted_v1/${slot}_female_ranger.glb`])),
  ...Object.fromEntries(['top', 'bottom', 'boots', 'gloves', 'helmet'].map(slot =>
    [`bard_${slot}`, `assets/items/female_bard_outfit/${slot}/fitted_v1/${slot}_female_bard.glb`])),
  valkyrie_hair: 'assets/modular_human_female_01/hair/tripo_long_side_v3/fitted_v1/hair_female_long_side.glb',
  long_bangs_hair: 'assets/modular_human_female_01/hair/tripo_long_bangs_v2/fitted_v1/hair_female_long_bangs.glb',
  valkyrie_wings: 'assets/items/valkyrie_starter/tripo_wing_v1/fitted_patch_v3/wings-armor-fit.glb',
  valkyrie_top: 'assets/items/valkyrie_starter/tripo_top_v1/fitted_v11/top_female_valkyrie.glb',
  valkyrie_bottom: 'assets/items/valkyrie_starter/tripo_bottom_v1/fitted_v8/bottom_female_valkyrie.glb',
  valkyrie_boots: 'assets/items/valkyrie_boots/fitted_v1/boots_female_valkyrie.glb',
  valkyrie_gloves: 'assets/items/valkyrie_gloves/fitted_v3/bracers_female_valkyrie.glb',
  valkyrie_helmet: 'assets/items/valkyrie_helmet/fitted_v2/helmet_female_valkyrie.glb',
  brilliance_helmet: 'assets/items/helm_of_brilliance/tripo_v2/fitted_v3/female/helmet_brilliance.glb',
}
// Body colour and ORM maps matched to the face's skin (tools/match-female-body-skin.py); female-base.glb keeps its own.
const bodySkin = { color: 'assets/modular_human_female_01/rig_v1/skin_tone_v1/body-albedo.png', orm: 'assets/modular_human_female_01/rig_v1/skin_tone_v1/body-orm.png' }
function matchBodySkin(doc) {
  const material = doc.getRoot().listMeshes()[0].listPrimitives()[0].getMaterial()
  material.getBaseColorTexture().setImage(readFileSync(bodySkin.color)).setMimeType('image/png')
  material.getMetallicRoughnessTexture().setImage(readFileSync(bodySkin.orm)).setMimeType('image/png')
}
const packs = ['locomotion', 'combat_melee', 'social', 'fishing', 'combat_ranged', 'offhand', 'enchant_armor', 'great_sword', 'riding', 'dagger_preview', 'cart_push']
const bakeExisting = process.argv.includes('--bake-existing')
const packFlag = process.argv.indexOf('--pack')
const selectedPack = packFlag === -1 ? undefined : process.argv[packFlag + 1]
if (selectedPack !== undefined && !packs.includes(selectedPack)) throw new Error(`--pack requires one of: ${packs.join(', ')}`)
const builtPacks = selectedPack ? [selectedPack] : packs
// Rebuilds one part without re-baking packs; only valid for parts the bakes do not read.
const partFlag = process.argv.indexOf('--part')
const selectedPart = partFlag === -1 ? undefined : process.argv[partFlag + 1]
if (selectedPart !== undefined && !(selectedPart in sources)) throw new Error(`--part requires one of: ${Object.keys(sources).join(', ')}`)
mkdirSync(`${output}/animations`, { recursive: true })
const reportPath = 'doc/assets/female-knight-runtime-sources.json'
const report = { date: '2026-11-03', inputs: {}, outputs: {} }
const record = (path) => { report.inputs[path] = hash(path) }
const helmetHairPath = 'doc/assets/female-helmet-hair-brush.json'
const { helmets: helmetHair } = await loadHelmetHairBrush()
const hairTriangles = new Map()
async function hairTriangleCount(hair) {
  if (!hairTriangles.has(hair))
    hairTriangles.set(hair, (await io.read(sources[hair])).getRoot().listMeshes()[0].listPrimitives()[0].getIndices().getCount() / 3)
  return hairTriangles.get(hair)
}
function writeReport() {
  const previous = existsSync(reportPath) ? JSON.parse(readFileSync(reportPath)) : {}
  if (bakeExisting || selectedPack || selectedPart) report.inputs = { ...previous.inputs, ...report.inputs }
  if (selectedPart) report.outputs = { ...previous.outputs, ...report.outputs }
  writeFileSync(reportPath, JSON.stringify({ ...previous, ...report }, null, 2) + '\n')
}
for (const path of ['tools/prepare-female-knight.mjs', 'client/src/lib/utils/femaleCharacter.ts', 'client/src/lib/utils/female-clip-bake.ts', 'client/src/lib/utils/female-boot-ground.ts']) record(path)
await Promise.all([MeshoptEncoder.ready, MeshoptDecoder.ready])
const io = new NodeIO().registerExtensions(ALL_EXTENSIONS)
  .registerDependencies({ 'meshopt.encoder': MeshoptEncoder, 'meshopt.decoder': MeshoptDecoder })
// Replaces the file instead of rewriting it, since worktrees may hard-link these GLBs.
async function write(path, doc, pack = false) {
  const bottom = path === `${output}/bottom.glb`
  // Packs share each clip's key times.
  if (pack) await doc.transform(dedup())
  if (path === `${output}/base.glb` || bottom || pack || separatePart(path.slice(output.length + 1))) {
    await doc.transform(prune({ propertyTypes: [PropertyType.MATERIAL, PropertyType.TEXTURE] }))
    // Preserve vertex and triangle numbering for outfit masks; do not quantize.
    doc.createExtension(EXTMeshoptCompression).setRequired(true)
      .setEncoderOptions({ method: EXTMeshoptCompression.EncoderMethod.QUANTIZE })
  }
  // Pelt rigs rewrite positions in place, which interleaved attributes do not support.
  const pelt = doc.getRoot().listNodes().some(node => node.getExtras().pelt_physics)
  io.setVertexLayout(pelt || bottom || pack ? VertexLayout.SEPARATE : VertexLayout.INTERLEAVED)
  writeFileSync(`${path}.tmp`, await io.writeBinary(doc))
  renameSync(`${path}.tmp`, path)
}
sharp.cache(false)
sharp.concurrency(2)
// --base-textures swaps only the body's colour and ORM maps in the built base.glb, with the full build's
// sizes and encoding; geometry, rig and baked clips stay byte-for-value the same.
if (process.argv.includes('--base-textures')) {
  const path = `${output}/base.glb`
  const doc = await io.read(path)
  for (const source of Object.values(bodySkin)) record(source)
  matchBodySkin(doc)
  const material = doc.getRoot().listMeshes()[0].listPrimitives()[0].getMaterial()
  for (const [texture, size, options] of [[material.getBaseColorTexture(), 2048, { quality: 94, effort: 5 }],
                                          [material.getMetallicRoughnessTexture(), 1024, { quality: 90, smartSubsample: true, effort: 5 }]])
    texture.setImage(await sharp(texture.getImage()).resize({ width: size, height: size, fit: 'inside', withoutEnlargement: true })
      .webp(options).toBuffer()).setMimeType('image/webp')
  await write(path, doc)
  report.outputs[path] = hash(path)
  const previous = JSON.parse(readFileSync(reportPath))
  writeFileSync(reportPath, JSON.stringify({ ...previous, inputs: { ...previous.inputs, ...report.inputs }, outputs: { ...previous.outputs, ...report.outputs } }, null, 2) + '\n')
  console.log(path)
  process.exit(0)
}
for (const [name, source] of Object.entries(sources)) {
  record(source)
  if (bakeExisting || selectedPack || (selectedPart && name !== selectedPart)) continue
  const doc = await io.read(source)
  const scene = doc.getRoot().listScenes()[0]
  const metadata = { ...scene.getExtras(), female_part: name }
  if (!/^(base|((barbarian|valkyrie|long_bangs)_)?hair|valkyrie_wings|rogue_garters)$/.test(name)) {
    const path = name === 'valkyrie_top' ? source.replace('fitted_v11/top_female_valkyrie.glb', 'fitted_v8/body-occlusion.json') : source.replace(/[^/]+$/, 'body-occlusion.json')
    record(path)
    metadata.body_mask = JSON.parse(readFileSync(path))
  }
  if (helmetHair[name]) {
    record(helmetHairPath)
    metadata.hair_masks = {}
    for (const [hair, hide] of Object.entries(helmetHair[name]))
      metadata.hair_masks[hair] = { mesh_index: 0, primitive_index: 0, primitive_triangles: await hairTriangleCount(hair),
                                    hide_triangle_indices: hide }
  }
  if (name === 'gloves') {
    const path = source.replace(/[^/]+$/, 'top-occlusion.json')
    record(path)
    metadata.sleeve_mask = JSON.parse(readFileSync(path))
  }
  scene.setExtras(metadata)
  if (name === 'base') {
    for (const node of doc.getRoot().listNodes()) {
      const mesh = node.getMesh()
      if (mesh) node.setExtras({ ...node.getExtras(), female_body: mesh === doc.getRoot().listMeshes()[0] })
    }
  }
  for (const mesh of doc.getRoot().listMeshes()) {
    for (const primitive of mesh.listPrimitives()) {
      const joints = primitive.getAttribute('JOINTS_0')
      const weights = primitive.getAttribute('WEIGHTS_0')
      if (!joints || !weights) continue
      const indices = joints.getArray().slice()
      for (let i = 0; i < indices.length; i++) if (weights.getArray()[i] === 0) indices[i] = 0
      joints.setArray(indices)
    }
  }
  if (name === 'base') {
    for (const path of Object.values(bodySkin)) record(path)
    matchBodySkin(doc)
  }
  const colors = new Set(doc.getRoot().listMaterials().map(m => m.getBaseColorTexture()))
  for (const texture of doc.getRoot().listTextures()) {
    if (/eyelash/i.test(texture.getName())) continue
    const color = colors.has(texture)
    const size = color && !separatePart(name) && !['bottom', 'rogue_bottom', 'rogue_top', 'barbarian_helmet'].includes(name) ? 2048 : 1024
    const image = await sharp(texture.getImage()).resize({ width: size, height: size, fit: 'inside', withoutEnlargement: true })
      .webp(color ? { quality: 94, effort: 5 } : { quality: 90, smartSubsample: true, effort: 5 }).toBuffer()
    texture.setImage(image).setMimeType('image/webp')
  }
  doc.createExtension(EXTTextureWebP).setRequired(true)
  if (name === 'barbarian_bottom') bakeSkirtRobe(doc)
  // Fitting sources carry unreferenced accessors from earlier iterations.
  await doc.transform(prune({ propertyTypes: [PropertyType.ACCESSOR], keepAttributes: true, keepIndices: true }))
  const path = `${output}/${name}.glb`
  await write(path, doc)
  console.log(path)
}
// Bakes from the written GLBs so the values match what the client loads; companion animations carry the boots variants.
async function bakeBootsAndGrip(module, load) {
  const female = await module('/src/lib/utils/femaleCharacter.ts')
  const { bakeFemaleClips } = await module('/src/lib/utils/female-clip-bake.ts')
  const partNames = [...new Set([...female.FEMALE_PARTS, ...female.FEMALE_BOOTS_PARTS])]
  const [base, ...parts] = await Promise.all(['base', ...partNames].map(name => load(`${output}/${name}.glb`)))
  const scene = female.assembleFemaleCharacter(base, parts, partNames)
  // The baked idle1 matches the source idle_armed, so base.glb ships only idle1.
  const grip = Object.fromEntries((await load(sources.base)).animations.find(clip => clip.name === 'idle_armed').tracks
    .filter(track => /^RightHand(Thumb|Index|Middle|Ring|Pinky)[123]\.quaternion$/.test(track.name))
    .map(track => [track.name.split('.')[0], Array.from(track.values.slice(0, 4))]))
  assert.equal(Object.keys(grip).length, 15)
  for (const path of [...(selectedPack ? [] : [`${output}/base.glb`]), ...builtPacks.map(name => `${output}/animations/${name}.glb`)]) {
    const gltf = path.endsWith('/base.glb') ? base : await load(path)
    const bakes = bakeFemaleClips(scene, gltf.animations.filter(clip => !clip.name.includes('|femaleBoots:') && clip.name !== 'idle_armed'), grip)
    const doc = await io.read(path)
    const root = doc.getRoot()
    const buffer = root.listBuffers()[0]
    const nodes = new Map(root.listNodes().map(node => [THREE.PropertyBinding.sanitizeNodeName(node.getName()), node]))
    const accessor = (type, array) => doc.createAccessor().setType(type).setArray(array).setBuffer(buffer)
    for (const animation of root.listAnimations())
      if (animation.getName().includes('|femaleBoots:') || animation.getName() === 'idle_armed') animation.dispose()
    const animations = root.listAnimations()
    assert.deepEqual(animations.map(a => a.getName()), bakes.map(b => b.name))
    for (const [i, animation] of animations.entries()) {
      const channel = track => {
        const [name, property] = track.name.split('.')
        const target = { quaternion: 'rotation', position: 'translation' }[property]
        const node = nodes.get(name)
        assert.ok(node && target, track.name)
        return { node, target, found: animation.listChannels().find(c => c.getTargetNode() === node && c.getTargetPath() === target) }
      }
      for (const track of bakes[i].grip) {
        const { node, target, found } = channel(track)
        const sampler = doc.createAnimationSampler().setInterpolation('LINEAR')
          .setInput(accessor('SCALAR', new Float32Array(track.times))).setOutput(accessor('VEC4', new Float32Array(track.values)))
        animation.addSampler(sampler)
        if (found) {
          const old = found.getSampler()
          found.setSampler(sampler)
          disposeSampler(old)
        } else animation.addChannel(doc.createAnimationChannel().setTargetNode(node).setTargetPath(target).setSampler(sampler))
      }
      for (const [boots, tracks] of Object.entries(bakes[i].boots)) {
        const companion = doc.createAnimation(female.femaleBootsClipName(animation.getName(), boots))
        for (const track of tracks) {
          const { node, target, found } = channel(track)
          assert.ok(found, track.name)
          const bare = found.getSampler()
          const sampler = doc.createAnimationSampler().setInterpolation(bare.getInterpolation())
            .setInput(bare.getInput()).setOutput(accessor(bare.getOutput().getType(), new Float32Array(track.values)))
          companion.addSampler(sampler).addChannel(doc.createAnimationChannel().setTargetNode(node).setTargetPath(target).setSampler(sampler))
        }
      }
    }
    // Replaced companion clips leave their keyframe accessors behind.
    await doc.transform(prune({ propertyTypes: [PropertyType.ACCESSOR], keepAttributes: true, keepIndices: true }))
    await write(path, doc, path.includes('/animations/'))
    console.log(`${path}: baked ${bakes.length} clips`)
  }
}

function disposeSampler(sampler) {
  const accessors = [sampler.getInput(), sampler.getOutput()]
  sampler.dispose()
  for (const accessor of accessors) if (accessor.listParents().every(parent => parent.propertyType === 'Root')) accessor.dispose()
}

if (selectedPart) {
  const path = `${output}/${selectedPart}.glb`
  report.outputs[path] = hash(path)
  writeReport()
  process.exit(0)
}
await ensureFemalePreview('animations')
const preparedPath = previewOutput('animations')
for (const path of dependencies('animations')) record(path)
const { server, load } = await headlessThree({ meshopt: true })
globalThis.FileReader = class {
  readAsArrayBuffer(blob) { blob.arrayBuffer().then(result => { this.result = result; this.onloadend?.() }) }
}
try {
  const runtime = await server.ssrLoadModule('/src/lib/utils/characterAnimationUtils.ts')
  const base = await load(`${output}/base.glb`)
  const corrected = new Map((await load(sources.base)).animations.map(c => [c.name, c]))
  for (const data of JSON.parse(readFileSync(preparedPath)).clips) {
    const clip = THREE.AnimationClip.parse(data)
    corrected.set(clip.name, clip)
  }
  for (const name of ['idle2', 'idle3', 'idle4', 'idle5']) {
    const clip = corrected.get('idle1').clone(); clip.name = name; corrected.set(name, clip)
  }
  for (const name of bakeExisting ? [] : builtPacks) {
    const source = `client/public/models/animations/${name}.glb`
    record(source)
    const pack = await load(source)
    const missing = pack.animations.filter(c => !corrected.has(c.name))
    let clips = await runtime.retargetAnimationsForCharacterModel(base.scene, pack.scene, missing)
    if (name !== 'riding') clips = await runtime.groundRetargetedClips(base.scene, clips)
    const byName = new Map(clips.map(c => [c.name, c]))
    const animations = pack.animations.map(c => corrected.get(c.name) ?? byName.get(c.name))
    const scene = base.scene.clone(true)
    const meshes = []; scene.traverse(n => { if (n.isMesh) meshes.push(n) })
    for (const mesh of meshes) mesh.removeFromParent()
    const bytes = await new GLTFExporter().parseAsync(scene, { binary: true, animations })
    const path = `${output}/animations/${name}.glb`
    const doc = await io.readBinary(new Uint8Array(bytes))
    await write(path, doc)
    console.log(`${name}: ${animations.length} clips`)
  }
  await bakeBootsAndGrip(path => server.ssrLoadModule(path), load)
} finally { await server.close() }
for (const path of [...Object.keys(sources).map(name => `${output}/${name}.glb`), ...packs.map(name => `${output}/animations/${name}.glb`)]) report.outputs[path] = hash(path)
writeReport()
