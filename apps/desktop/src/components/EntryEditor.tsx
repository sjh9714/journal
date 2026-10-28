import { useEffect, useRef, useState } from "react";
import { EditorContent, useEditor } from "@tiptap/react";
import StarterKit from "@tiptap/starter-kit";
import Image from "@tiptap/extension-image";
import Placeholder from "@tiptap/extension-placeholder";
import { Markdown } from "@tiptap/markdown";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { Cloud, FolderOpen, ImagePlus, MapPin, Star, Trash2 } from "lucide-react";
import { api, IN_TAURI } from "../api";
import type { Entry } from "../types";
import { dayLabel, timeLabel } from "../lib/dates";
import { attachmentUrl, toDisk, toEditor } from "../lib/markdown";
import { TagInput } from "./TagInput";

interface Props {
  entry: Entry;
  /** Change this to force the editor to reload `entry.body`. */
  reloadKey: number;
  journals: string[];
  onChanged: (e: Entry) => void;
  onDeleted: (id: string) => void;
}

export function EntryEditor({ entry, reloadKey, journals, onChanged, onDeleted }: Props) {
  const [saved, setSaved] = useState<"saved" | "saving" | "dirty">("saved");
  const timer = useRef<number | null>(null);
  const current = useRef(entry.uuid);
  const loadedKey = useRef(reloadKey);

  const editor = useEditor({
    extensions: [
      StarterKit,
      Markdown,
      Image.configure({ inline: false }),
      Placeholder.configure({ placeholder: "What happened today?" }),
    ],
    content: toEditor(entry.body),
    contentType: "markdown",
    autofocus: "end",
    onUpdate: ({ editor }) => {
      setSaved("dirty");
      if (timer.current) window.clearTimeout(timer.current);
      timer.current = window.setTimeout(async () => {
        setSaved("saving");
        const id = current.current;
        const body = toDisk(editor.getMarkdown());
        try {
          const e = await api.update(id, { body });
          if (current.current === id) {
            onChanged(e);
            setSaved("saved");
          }
        } catch {
          setSaved("dirty");
        }
      }, 600);
    },
  });

  // Reload the document when a different entry is selected or the same one is re-opened.
  useEffect(() => {
    if (!editor) return;
    if (current.current !== entry.uuid || loadedKey.current !== reloadKey) {
      if (timer.current) window.clearTimeout(timer.current);
      current.current = entry.uuid;
      loadedKey.current = reloadKey;
      editor.commands.setContent(toEditor(entry.body), { emitUpdate: false, contentType: "markdown" });
      setSaved("saved");
      editor.commands.focus("end");
    }
  }, [entry.uuid, entry.body, reloadKey, editor]);

  // Flush pending save on unmount.
  useEffect(() => () => { if (timer.current) window.clearTimeout(timer.current); }, []);

  const insertFiles = async (paths: string[]) => {
    if (!editor) return;
    for (const p of paths) {
      if (!/\.(png|jpe?g|gif|webp|heic|svg)$/i.test(p)) continue;
      const a = await api.attachFile(p);
      editor.chain().focus().setImage({ src: attachmentUrl(a.rel) }).run();
    }
  };

  useEffect(() => {
    if (!IN_TAURI) return;
    let unlisten: (() => void) | undefined;
    getCurrentWebview().onDragDropEvent((ev) => {
      if (ev.payload.type === "drop") insertFiles(ev.payload.paths);
    }).then((u) => (unlisten = u));
    return () => unlisten?.();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [editor]);

  const patch = async (p: Parameters<typeof api.update>[1]) => onChanged(await api.update(entry.uuid, p));
  const addImage = async () => {
    if (!IN_TAURI) return;
    const f = await open({ multiple: true, filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "gif", "webp", "heic"] }] });
    if (Array.isArray(f)) insertFiles(f);
    else if (typeof f === "string") insertFiles([f]);
  };
  const remove = async () => {
    // Soft delete: the file moves to the trash and the app offers Undo.
    await api.remove(entry.uuid);
    onDeleted(entry.uuid);
  };
  const reveal = async () => {
    if (!IN_TAURI) return;
    revealItemInDir(await api.entryAbsPath(entry.uuid));
  };

  return (
    <div className="editor-pane">
      <header className="entry-head" data-tauri-drag-region>
        <div>
          <div className="entry-date">{dayLabel(entry.created.slice(0, 10))}</div>
          <div className="entry-meta">
            <span>{timeLabel(entry.created)}</span>
            {entry.location && (
              <span title={`${entry.location.latitude}, ${entry.location.longitude}`}><MapPin size={12} /> {entry.location.place_name ?? "Location saved"}</span>
            )}
            {entry.weather && (entry.weather.temperature_c != null || entry.weather.conditions) && (
              <span><Cloud size={12} /> {entry.weather.conditions ?? ""}{entry.weather.temperature_c != null ? ` ${Math.round(entry.weather.temperature_c)}°` : ""}</span>
            )}
            <select
              className="journal-select"
              value={entry.journal}
              onChange={(e) => {
                const v = e.target.value;
                if (v !== "__new") return void patch({ journal: v });
                const name = prompt("Name for the new journal")?.trim();
                if (name) patch({ journal: name });
              }}
            >
              {[...new Set([entry.journal, ...journals])].map((j) => <option key={j} value={j}>{j}</option>)}
              <option value="__new">New journal…</option>
            </select>
            <span className={`save ${saved}`}>{saved === "saved" ? "Saved" : saved === "saving" ? "Saving…" : "Editing"}</span>
          </div>
        </div>
        <div className="entry-actions">
          <button className={entry.starred ? "icon-btn on" : "icon-btn"} title="Star" onClick={() => patch({ starred: !entry.starred })}><Star size={16} /></button>
          {IN_TAURI && <button className="icon-btn" title="Add image" onClick={addImage}><ImagePlus size={16} /></button>}
          {IN_TAURI && <button className="icon-btn" title="Show file" onClick={reveal}><FolderOpen size={16} /></button>}
          <button className="icon-btn danger" title="Move to Trash" onClick={remove}><Trash2 size={16} /></button>
        </div>
      </header>
      <div className="entry-body">
        <EditorContent editor={editor} className="prose" />
      </div>
      <footer className="entry-foot">
        <TagInput tags={entry.tags} onChange={(tags) => patch({ tags })} />
      </footer>
    </div>
  );
}
