export function findBestMatchingModel<T extends { slug: string }>(
  models: readonly T[],
  modelSlug: string,
): T | null {
  const requestedSlug = String(modelSlug || "").trim();
  if (!requestedSlug) return null;

  let bestMatch: T | null = null;
  for (const model of models) {
    const candidateSlug = String(model.slug || "").trim();
    if (!candidateSlug || !requestedSlug.startsWith(candidateSlug)) continue;
    if (
      bestMatch == null ||
      candidateSlug.length > String(bestMatch.slug || "").trim().length
    ) {
      bestMatch = model;
    }
  }
  return bestMatch;
}
