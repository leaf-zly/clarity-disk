import { beforeEach, vi } from "vitest";
import { version } from "../../package.json";

// Mirror Vite's version injection while allowing each test to emulate older clients.
beforeEach(() => vi.stubGlobal("__APP_VERSION__", version));
