/* @refresh reload */
import { render } from "solid-js/web";
import { Router, Route } from "@solidjs/router";
import "./app.css";
import App from "./App";
import Reader from "./routes/Reader";
import ArticleView from "./routes/ArticleView";
import Search from "./routes/Search";
import Settings from "./routes/Settings";
import FeedManage from "./routes/FeedManage";
import Digest from "./routes/Digest";
import Clusters from "./routes/Clusters";
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
        component={Reader}
      />
      <Route path="/articles/:id" component={ArticleView} />
      <Route path="/search" component={Search} />
      <Route path="/manage" component={FeedManage} />
      <Route path="/digest" component={Digest} />
      <Route path="/clusters" component={Clusters} />
      <Route path="/settings" component={Settings} />
      <Route path="*" component={NotFound} />
    </Router>
  ),
  root,
);
