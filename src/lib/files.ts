import { convertFileSrc } from "@tauri-apps/api/core";

/** Read browser File objects as text (used for local-only imports). */
export const readFiles = (files: FileList | File[]) =>
  Promise.all(Array.from(files).map(async (f) => ({ name: (f as File & { webkitRelativePath?: string }).webkitRelativePath || f.name, content: await f.text() })));

export async function saveDialog(defaultPath: string, ext: string, label: string): Promise<string | null> {
  const { save } = await import("@tauri-apps/plugin-dialog");
  return save({ defaultPath, filters: [{ name: label, extensions: [ext] }] });
}

export async function openDialog(opts: { multiple?: boolean; extensions?: string[]; label?: string }): Promise<string[]> {
  const { open } = await import("@tauri-apps/plugin-dialog");
  const r = await open({ multiple: opts.multiple ?? false, filters: opts.extensions ? [{ name: opts.label ?? "Files", extensions: opts.extensions }] : undefined });
  if (!r) return [];
  return Array.isArray(r) ? r : [r];
}

export async function openExternal(url: string) {
  // Only web links are opened; anything else is ignored.
  if (!/^https:\/\//.test(url)) return;
  const { openUrl } = await import("@tauri-apps/plugin-opener");
  await openUrl(url);
}

export async function fileSrc(path: string) {
  return convertFileSrc(path);
}
