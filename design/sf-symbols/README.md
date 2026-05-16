# Custom SF Symbols

These SVGs are monochrome vector templates intended for import into the SF Symbols app as custom symbols.

Files:

- `private.svg`
- `feed.svg`
- `record.svg`

Suggested symbol names:

- `private`
- `feed`
- `record`

Quick import flow:

1. Open the SF Symbols app.
2. Create a new custom symbol set.
3. Import each SVG.
4. Export the symbol package into your Xcode project.

SwiftUI usage example:

```swift
Image(systemName: "private")
Image(systemName: "feed")
Image(systemName: "record")
```

Design intent:

- `private`: minimal shield + lock
- `feed`: streamlined panel + directional flow mark
- `record`: clean reticle + centered capture dot
