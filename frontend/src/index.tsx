/* @refresh reload */
import { render } from "solid-js/web";
import { Router, Route } from "@solidjs/router";
import "./app.css";
import App from "./App";
import ArticleList from "./routes/ArticleList";
import ArticleView from "./routes/ArticleView";
import Settings from "./routes/Settings";
import NotFound from "./routes/NotFound";
import { initTheme } from "./lib/theme";

const root = document.getElementById("root");
if (!root) throw new Error("#root not found");

initTheme(); // render 前に <html> へ dark クラス + color-scheme を同期適用

render(
  () => (
    <Router root={App}>
      <Route
        path={["/", "/feeds/:feedId", "/folders/:folderId"]}
        component={ArticleList}
      />
      <Route path="/articles/:id" component={ArticleView} />
      <Route path="/settings" component={Settings} />
      <Route path="*" component={NotFound} />
    </Router>
  ),
  root,
);
