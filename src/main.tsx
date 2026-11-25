import { createRoot } from "react-dom/client";
import { App } from "./App";
import "./styles.css";

// 不使用 StrictMode：开发模式下它会重复执行副作用，导致事件监听重复
createRoot(document.getElementById("root")!).render(<App />);
