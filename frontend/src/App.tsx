import type { ParentComponent } from "solid-js";

/** App shell: header + routed content. Kept deliberately minimal. */
const App: ParentComponent = (props) => {
  return (
    <div class="min-h-screen bg-background text-foreground">
      <header class="border-b border-border">
        <div class="mx-auto max-w-3xl px-4 py-3 flex items-center justify-between">
          <a href="/" class="text-lg font-semibold tracking-tight">
            RSS Reader
          </a>
          <span class="text-sm text-muted-foreground">self-hosted</span>
        </div>
      </header>
      <main class="mx-auto max-w-3xl px-4 py-6">{props.children}</main>
    </div>
  );
};

export default App;
