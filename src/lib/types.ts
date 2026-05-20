export type Surfaces = {
  gui: boolean
  cli: boolean
}

export type Profile = {
  id: string
  name: string
  slug: string
  color: string
  createdAt: string
  surfaces: Surfaces
}

export type AppError = {
  kind: 'Io' | 'Json' | 'Validation' | 'NotFound'
  message: string
}
