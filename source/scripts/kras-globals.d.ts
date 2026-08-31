type KrasHook = (this: KrasCertView, ...args: unknown[]) => unknown

type KrasCertView = {
  fnCheckLedger: KrasHook
  fnEmptyRequestInfo: KrasHook
}

declare const certView: KrasCertView | undefined
