import { useState } from "react";
import { X } from "lucide-react";

export function TagInput({ tags, onChange }: { tags: string[]; onChange: (tags: string[]) => void }) {
  const [draft, setDraft] = useState("");
  const commit = () => {
    const t = draft.trim().replace(/^#/, "");
    if (t && !tags.includes(t)) onChange([...tags, t].sort());
    setDraft("");
  };
  return (
    <div className="tags">
      {tags.map((t) => (
        <span key={t} className="tag">
          #{t}
          <button className="tag-x" aria-label={`remove ${t}`} onClick={() => onChange(tags.filter((x) => x !== t))}>
            <X size={11} />
          </button>
        </span>
      ))}
      <input
        className="tag-input"
        placeholder={tags.length ? "" : "Add tag"}
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === ",") {
            e.preventDefault();
            commit();
          } else if (e.key === "Backspace" && !draft && tags.length) {
            onChange(tags.slice(0, -1));
          }
        }}
        onBlur={commit}
      />
    </div>
  );
}
