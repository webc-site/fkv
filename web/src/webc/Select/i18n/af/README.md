# Aftreklys seleksie, breedte pas by huidige item aan

Die breedte van inheemse `select` word by verstek deur die langste opsieteks bepaal. Wanneer kort opsies verander word, is daar te veel wit spasie, wat die voorkoms beïnvloed.

Hierdie komponent dek deursigtig die oorspronklike `select` bokant die `b` merker, wat die breedte toelaat om akkuraat aan te pas by die lengte van die tans geselekteerde teks.

- Handhaaf inheemse `select` aftrekkieslysstyle en toeganklikheidsteun op alle terminale
- Ondersteun inheemse interaktiewe bedrywighede soos volledige sleutelbord Tab-navigasie en rigtingsleutelwisseling.
- Pas die breedte aan by die huidige opsie om oortollige wit spasie uit te skakel

## Gebruik die demo

```html
<c-select>
  <option value="js">JavaScript</option>
  <option value="rs">Rust</option>
  <option value="py">Python</option>
</c-select>
```