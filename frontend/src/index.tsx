/* @refresh reload */
import { render } from "solid-js/web";
import { Router, Route } from "@solidjs/router";
import "./app.css";
import App from "./App";
import FeedList from "./routes/FeedList";
import ArticleView from "./routes/ArticleView";
import { initTheme } from "./lib/theme";

const root = document.getElementById("root");
if (!root) throw new Error("#root not found");

initTheme(); // render 前に <html> へ dark クラス + color-scheme を同期適用

render(
  () => (
    <Router root={App}>
      <Route path="/" component={FeedList} />
      <Route path="/articles/:id" component={ArticleView} />
    </Router>
  ),
  root,
);
