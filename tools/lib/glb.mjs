import assert from 'node:assert/strict'

export function readGlb(bytes) {
  assert.equal(bytes.readUInt32LE(0), 0x46546c67)
  assert.equal(bytes.readUInt32LE(4), 2)
  assert.equal(bytes.readUInt32LE(16), 0x4e4f534a)
  const jsonLength = bytes.readUInt32LE(12), binStart = 20 + jsonLength
  const bin = bytes.length >= binStart + 8 && bytes.readUInt32LE(binStart + 4) === 0x004e4942
    ? bytes.subarray(binStart + 8, binStart + 8 + bytes.readUInt32LE(binStart)) : Buffer.alloc(0)
  return { json: JSON.parse(bytes.toString('utf8', 20, binStart)), bin }
}

export function writeGlb(json, binary) {
  const pad = (data, fill) => Buffer.concat([data, Buffer.alloc((4 - data.length % 4) % 4, fill)])
  const js = pad(Buffer.from(JSON.stringify(json)), 0x20), bin = pad(binary, 0)
  const chunk = (data, type) => { const h = Buffer.alloc(8); h.writeUInt32LE(data.length); h.writeUInt32LE(type, 4); return Buffer.concat([h, data]) }
  const header = Buffer.alloc(12); header.writeUInt32LE(0x46546c67); header.writeUInt32LE(2, 4); header.writeUInt32LE(12 + 8 + js.length + 8 + bin.length, 8)
  return Buffer.concat([header, chunk(js, 0x4e4f534a), chunk(bin, 0x004e4942)])
}

export function glbWriter(json, binary) {
  const chunks = [binary]; let offset = binary.length
  function addAccessor(values, type, extra = {}) {
    const padding = (4 - offset % 4) % 4
    if (padding) { chunks.push(Buffer.alloc(padding)); offset += padding }
    const data = Buffer.from(new Float32Array(values).buffer)
    json.bufferViews.push({ buffer: 0, byteOffset: offset, byteLength: data.length })
    chunks.push(data); offset += data.length
    json.accessors.push({ bufferView: json.bufferViews.length - 1, componentType: 5126, count: values.length / ({ SCALAR: 1, VEC3: 3, VEC4: 4 }[type]), type, ...extra })
    return json.accessors.length - 1
  }
  function finish() {
    json.buffers[0].byteLength = offset
    return writeGlb(json, Buffer.concat(chunks))
  }
  return { addAccessor, finish }
}
