<script setup lang="ts">
import { computed } from "vue";
import { Globe2, Moon, Trash2 } from "@lucide/vue";

import type { Suggestion, SuggestionRisk } from "@/types/dashboard";

/**
 * Props for a single actionable maintenance recommendation.
 */
interface Props {
  suggestion: Suggestion;
}

interface RiskPresentation {
  label: string;
  tone: "safe" | "review";
}

const props = defineProps<Props>();

const iconById = {
  hibernation: Moon,
  "browser-cache": Globe2,
  "recycle-bin": Trash2,
} as const;

const riskPresentation: Readonly<Record<SuggestionRisk, RiskPresentation>> = {
  safe: { label: "可安全清理", tone: "safe" },
  review: { label: "建议查看", tone: "review" },
  confirmationRequired: { label: "需要确认", tone: "review" },
};

const icon = computed(
  () => iconById[props.suggestion.id as keyof typeof iconById] ?? Trash2,
);
const risk = computed(() => riskPresentation[props.suggestion.risk]);
</script>

<template>
  <article class="suggestion">
    <span class="suggestion-icon" :class="risk.tone">
      <component :is="icon" :size="18" aria-hidden="true" />
    </span>
    <div class="suggestion-content">
      <h3>{{ suggestion.title }}</h3>
      <p>{{ suggestion.description }}</p>
    </div>
    <span class="suggestion-risk" :class="risk.tone">{{ risk.label }}</span>
  </article>
</template>

<style scoped>
.suggestion {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr) auto;
  align-items: center;
  gap: 13px;
  padding: 13px 0;
  border-bottom: 1px solid var(--color-border);
}

.suggestion:last-child {
  border-bottom: 0;
}

.suggestion-icon {
  width: 34px;
  height: 34px;
  display: grid;
  place-items: center;
  border-radius: 10px;
}

.suggestion-icon.safe {
  color: var(--color-green);
  background: var(--color-green-soft);
}

.suggestion-icon.review {
  color: var(--color-orange);
  background: var(--color-orange-soft);
}

.suggestion-content h3 {
  font-size: 0.95rem;
}

.suggestion-content p {
  margin-top: 3px;
  color: var(--color-text-secondary);
  font-size: 0.84rem;
}

.suggestion-risk {
  white-space: nowrap;
  font-size: 0.83rem;
}

.suggestion-risk.safe {
  color: var(--color-green);
}

.suggestion-risk.review {
  color: var(--color-orange);
}
</style>
