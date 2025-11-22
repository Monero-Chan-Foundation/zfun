import clsx from "clsx";
import { ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

import type { BalanceMatch } from "./types";

const NUMBER_MATCH = /^[+-]?(\d*[.,])?\d+(e[+-]?\d+)?$/i;

export function computeHintScore(key: string) {
  const normalized = key.toLowerCase();
  let score = 0;
  if (normalized.includes("balance")) score += 1;
  if (normalized.includes("shield")) score += 2;
  if (normalized.includes("sapling")) score += 2;
  if (normalized.includes("zec")) score += 1;
  if (normalized.includes("zcash")) score += 1;
  if (normalized.includes("private")) score += 1;
  if (normalized.includes("spendable")) score += 1;
  return score;
}

export function asNumber(candidate: unknown) {
  if (typeof candidate === "number" && Number.isFinite(candidate)) {
    return candidate;
  }

  if (typeof candidate === "string") {
    const trimmed = candidate.trim().replace(/[,_\s]/g, "");
    if (trimmed && NUMBER_MATCH.test(trimmed)) {
      const parsed = Number(trimmed.replace(/,/g, ""));
      if (!Number.isNaN(parsed)) {
        return parsed;
      }
    }
  }

  return null;
}

export function findShieldedBalances(input: unknown, path: string[] = [], score = 0): BalanceMatch[] {
  if (Array.isArray(input)) {
    return input.flatMap((value, index) => findShieldedBalances(value, [...path, String(index)], score));
  }

  if (input && typeof input === "object") {
    const matches: BalanceMatch[] = [];
    for (const [key, value] of Object.entries(input)) {
      const nextScore = score + computeHintScore(key);
      const nextPath = [...path, key];

      const candidateNumber = asNumber(value);
      if (candidateNumber !== null && nextScore >= 2) {
        matches.push({
          path: nextPath.join(" › "),
          value: candidateNumber,
          rawKey: key,
        });
        continue;
      }

      matches.push(...findShieldedBalances(value, nextPath, nextScore));
    }
    return matches;
  }

  const primitiveNumber = asNumber(input);
  if (primitiveNumber !== null && score >= 2) {
    return [
      {
        path: path.join(" › "),
        value: primitiveNumber,
        rawKey: path[path.length - 1] ?? "value",
      },
    ];
  }

  return [];
}

export function cn(...args: ClassValue[]) {
  return twMerge(clsx(args));
}

