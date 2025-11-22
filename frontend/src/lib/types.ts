import type React from "react";

export type BalanceMatch = {
  path: string;
  value: number;
  rawKey: string;
};

export type Analysis = {
  fileName: string;
  matches: BalanceMatch[];
  total: number;
};

export type StepIndex = 1 | 2 | 3 | 4;

export type StepDefinition = {
  id: StepIndex;
  label: string;
};

export type FaqItem = {
  question: string;
  paragraphs?: (string | React.ReactNode)[];
  orderedListTitle?: string;
  orderedList?: string[];
  unorderedListTitle?: string;
  unorderedList?: string[];
  concluding?: string | React.ReactNode;
};

