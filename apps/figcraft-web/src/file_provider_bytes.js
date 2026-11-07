// Browser hosts pass native snapshots across iframe realms. Inspect intrinsic typed-array state,
// not instanceof, user-defined tags, iterators, constructors, or length properties.
const typedArrayPrototype = Object.getPrototypeOf(Uint8Array.prototype);
const typeOf = Object.getOwnPropertyDescriptor(typedArrayPrototype, Symbol.toStringTag).get;
const byteLengthOf = Object.getOwnPropertyDescriptor(typedArrayPrototype, 'byteLength').get;
const setBytes = Uint8Array.prototype.set;

export function providerBytesLength(value, max) {
  if (!Number.isSafeInteger(max) || max < 1 || max > 80 * 1024 * 1024 ||
      !ArrayBuffer.isView(value) || typeOf.call(value) !== 'Uint8Array') {
    throw new TypeError('Provider bytes must be a Uint8Array.');
  }
  const length = byteLengthOf.call(value);
  if (length < 1 || length > max) throw new RangeError('File exceeds the provider limit.');
  return length;
}

export function copyProviderBytes(value, expectedLength, max) {
  const length = providerBytesLength(value, max);
  if (length !== expectedLength) throw new RangeError('Provider bytes changed before capture.');
  const owned = new Uint8Array(length);
  setBytes.call(owned, value);
  return owned;
}
