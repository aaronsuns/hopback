/**
 * Whether the Notifications step is done. macOS's answer wins; when it has not
 * been asked yet (`null`), sending a test banner counts.
 */
export function notificationsDone(allowed: boolean | null, testBannerSent: boolean): boolean {
  return allowed ?? testBannerSent;
}
