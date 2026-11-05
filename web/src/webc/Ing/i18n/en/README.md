# Indicator of loading or processing status

- Unify the cursor into waiting state
- Shows ripple animation to indicate processing status
- Apply a blur mask to a form to prevent duplicate submissions

## Use the demo

```html
<!-- Independent Loading Indicator -->
<div class="Ing"></div>

<!-- Loading form with mask -->
<form class="Ing">
  <input type="text" placeholder="Input box">
  <button type="submit">Submit</button>
</form>
```

## Style Class

### `.Ing`
- Pointer status is `wait`.
- Pseudo element `::before` renders a ripple animation.
- When used on the `form` element, the pseudo-element `::after` renders a 1px blur mask, disabling interaction and text selection.