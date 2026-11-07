# Copy text to clipboard

Click the button to copy the text of the specified element.

- Supports using rel to specify the element selector to be copied
- When not specified, the previous sibling element is copied by default.
- Nested Pophover prompt box
- International support

## Use the demo

```html
<!-- Copy specific elements -->
<span id="target">Here is the text to be copied</span>
<c-copy rel="#target"></c-copy>

<!-- Preceding elements are copied by default -->
<span>the text of the previous element</span>
<c-copy></c-copy>
```

## Parameter

| Property | Readme |
| --- | --- |
| rel | Copy the selector of the target element. Optional |

## Status Style

- `c-pophover.Done`: prompt box style after successful copying