import { createSignal } from "solid-js";
import { useNavigate } from "@solidjs/router";
import { Input } from "@/components/ui/input";
import { searchHref } from "@/lib/search";

/** Sidebar の検索フォーム。submit で /search?q=… へ遷移するだけの薄い部品。 */
export function SearchBox(props: { onNavigate?: () => void }) {
  const navigate = useNavigate();
  const [value, setValue] = createSignal("");

  const submit = (e: Event) => {
    e.preventDefault();
    const href = searchHref(value());
    if (!href) return; // 空クエリは遷移しない
    navigate(href);
    props.onNavigate?.();
  };

  return (
    <form role="search" onSubmit={submit}>
      <Input
        type="search"
        name="q"
        placeholder="記事を検索…"
        aria-label="記事を検索"
        value={value()}
        onInput={(e) => setValue(e.currentTarget.value)}
      />
    </form>
  );
}
