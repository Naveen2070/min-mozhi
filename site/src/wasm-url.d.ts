// Vite resolves `?url` imports to the emitted asset URL (used to locate the
// playground's `mimz_wasm_bg.wasm` at runtime).
declare module "*.wasm?url" {
  const src: string;
  export default src;
}
