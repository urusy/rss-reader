/* @refresh reload */
import { render } from "solid-js/web";
import { Router, Route } from "@solidjs/router";
import "./app.css";
import App from "./App";
import FeedList from "./routes/FeedList";
import ArticleView from "./routes/ArticleView";

const root = document.getElementById("root");
if (!root) throw new Error("#root not found");

render(
  () => (
    <Router root={App}>
      <Route path="/" component={FeedList} />
      <Route path="/articles/:id" component={ArticleView} />
    </Router>
  ),
  root,
);
