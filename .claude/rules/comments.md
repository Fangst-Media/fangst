# Comment Standards

Comments should explain WHY, not WHAT. The code already explains WHAT it does.

## Bad Examples
```typescript
// Loop through users
for (const user of users) {

// Check if admin
if (user.role === 'admin') {

// Extract resolution
const match = name.match(/1080p/)
```

## Good Examples
```typescript
// Process users in order to maintain referential integrity
for (const user of users) {

// Admins bypass rate limiting
if (user.role === 'admin') {

// Atmos must be checked separately as it appears alongside other audio codecs
if (/\batmos\b/i.test(name)) {
```

## When to Comment
- Non-obvious business logic decisions
- Performance considerations
- Security implications
- Workarounds for bugs or limitations
- Complex algorithms that aren't immediately clear

## When NOT to Comment
- Function/variable names that are self-explanatory
- Basic language constructs (loops, conditionals)
- Obvious patterns (getters, setters, CRUD operations)
