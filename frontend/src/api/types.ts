// Hand-written until the backend exists; step 5 replaces this with types
// generated from /api/openapi.json into schema.d.ts.
export type Me = {
  name: string | null
  email: string | null
}
