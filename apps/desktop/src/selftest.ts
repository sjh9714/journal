// Dev-only end-to-end check that runs inside the real webview.
// Enabled with VITE_SELFTEST=1; VITE_SELFTEST_IMG points at an image on disk.
import { api } from "./api";
import { todayISO } from "./lib/dates";
import { report } from "./lib/log";
import { attachmentUrl } from "./lib/markdown";

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

export async function runSelfTest(open: (id: string) => Promise<void>) {
  const out: string[] = [];
  const check = (name: string, ok: unknown, extra = "") => out.push(`${ok ? "PASS" : "FAIL"} ${name}${extra ? `: ${extra}` : ""}`);
  try {
    const st = await api.status();
    check("status.root", st.root, st.root ?? "");
    const made = await api.create({ body: "# Selftest\n\nhello selftest", tags: ["selftest"] });
    check("create tags", made.tags.includes("selftest"));
    const plain = await api.create({ body: "tagless" });
    check("create tagless has []", Array.isArray(plain.tags) && plain.starred === false);
    await open(plain.uuid);
    await sleep(700);
    check("editor mounted (tagless)", document.querySelector(".tiptap"));
    await open(made.uuid);
    await sleep(700);
    const tt = document.querySelector(".tiptap")?.textContent ?? "";
    check("editor shows body", tt.includes("hello selftest"), tt.slice(0, 40));
    check("heading rendered", document.querySelector(".tiptap h1"));
    const img = import.meta.env.VITE_SELFTEST_IMG as string | undefined;
    if (img) {
      const a = await api.attachFile(img);
      check("attach rel", a.rel.startsWith("attachments/"), a.rel);
      const url = attachmentUrl(a.rel);
      const loaded = await new Promise<boolean>((r) => {
        const el = new Image();
        el.onload = () => r(el.naturalWidth > 0);
        el.onerror = () => r(false);
        el.src = url;
      });
      check("dayfile:// image loads", loaded, url);
      const upd = await api.update(made.uuid, { body: `${made.body}\n![](${a.link})\n`, starred: true });
      check("update starred", upd.starred);
      await open(upd.uuid);
      await sleep(900);
      const imgEl = document.querySelector<HTMLImageElement>(".tiptap img");
      check("editor renders image", imgEl && imgEl.naturalWidth > 0, imgEl?.src ?? "no img");
    }
    const hits = await api.search("selftest");
    check("search", hits.some((h) => h.uuid === made.uuid));
    check("on_this_day", Array.isArray(await api.onThisDay(todayISO())));
    check("tags listed", (await api.tags()).some(([t]) => t === "selftest"));
    const geo = "geolocation" in navigator;
    check("navigator.geolocation present", geo);
    for (const id of [made.uuid, plain.uuid]) {
      const del = await api.remove(id);
      check("delete", del.uuid === id);
    }
  } catch (e) {
    out.push(`FAIL exception: ${e instanceof Error ? e.stack ?? e.message : String(e)}`);
  }
  report("warn", `SELFTEST\n${out.join("\n")}\nSELFTEST END`);
}
