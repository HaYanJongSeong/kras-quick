import { z } from "zod"

const propertyRowSchema = z
  .tuple([
    z.string().max(4096),
    z.string().max(4096),
    z.string().max(4096),
    z.string().max(4096),
    z.string().max(4096),
    z.string().max(4096),
    z.string().max(4096),
    z.string().max(4096),
  ])
  .readonly()

const PAGER_PATTERN = /^([1-9]\d*)\/([1-9]\d*)$/u

const propertyPagerSchema = z
  .string()
  .regex(PAGER_PATTERN)
  .superRefine((value, context) => {
    const match = PAGER_PATTERN.exec(value)
    if (match === null) return
    const currentText = match[1]
    const totalText = match[2]
    if (currentText === undefined || totalText === undefined) return
    const current = Number(currentText)
    const total = Number(totalText)
    if (!Number.isSafeInteger(current) || !Number.isSafeInteger(total) || current > total) {
      context.addIssue({ code: "custom", message: "Invalid property pager." })
    }
  })
  .brand("PropertyPager")

const propertyWatcherPayloadSchema = z
  .object({
    version: z.literal(3),
    selectionId: z.number().int().positive().max(Number.MAX_SAFE_INTEGER),
    cells: propertyRowSchema,
    pager: propertyPagerSchema,
  })
  .strict()
  .readonly()

export type PropertyRow = z.infer<typeof propertyRowSchema>
export type PropertyPager = z.infer<typeof propertyPagerSchema>
export type PropertyWatcherSelection = z.infer<typeof propertyWatcherPayloadSchema>
export type PropertyWatcherSelectionHandle = {
  readonly contextId: number
  readonly contextUniqueId: string
  readonly generation: number
  readonly selectionId: number
}
export type PropertyBuildingTerminalStatus = "absent" | "failed" | "present"
export type PropertyBuildingStatus = "pending" | PropertyBuildingTerminalStatus
export type PropertyBuildingStatusUpdateResult = "applied" | "stale"

export function parsePropertyPager(value: unknown): PropertyPager | undefined {
  const parsed = propertyPagerSchema.safeParse(value)
  return parsed.success ? parsed.data : undefined
}

export function parsePropertyWatcherPayload(payload: string): PropertyWatcherSelection | undefined {
  if (Buffer.byteLength(payload, "utf8") > 65_536) return undefined
  let decoded: unknown
  try {
    decoded = JSON.parse(payload)
  } catch (error) {
    if (error instanceof SyntaxError) return undefined
    throw error
  }
  const parsed = propertyWatcherPayloadSchema.safeParse(decoded)
  return parsed.success ? parsed.data : undefined
}
