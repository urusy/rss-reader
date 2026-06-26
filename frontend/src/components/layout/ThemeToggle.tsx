import { Switch } from "@/components/ui/switch";
import { theme, setTheme } from "@/lib/theme";

export function ThemeToggle() {
  return (
    <Switch
      label="ダークモード"
      checked={theme() === "dark"}
      onCheckedChange={(e) => setTheme(e.checked ? "dark" : "light")}
    />
  );
}
