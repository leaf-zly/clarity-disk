import { createApp } from "vue";

import App from "./App.vue";
import { getAppSettings } from "@/services/operations-service";
import { applyThemePreference } from "@/services/theme-service";
import "./styles/base.css";

// Establish a deterministic system theme before Vue paints, then replace it
// with the persisted preference as soon as the local Tauri command responds.
applyThemePreference("system");
createApp(App).mount("#app");
void getAppSettings()
  .then((settings) => applyThemePreference(settings.theme))
  .catch(() => {
    // Safe fallback remains the operating-system preference.
  });
