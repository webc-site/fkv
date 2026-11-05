# Beira izoztuaren atzeko planoa eta ertz nabarmentzeko efektua

Ertz erdi gardenak eta barruko itzalak mikro-erliebearen ertz nabarmenak sortzen dituzte.
Botoiaren eta sarrera-koadroaren osagaien oinarrizko estilo-mendekotasun gisa.

## Erabili demoa

```html
<main class="Lg">
  <article>
    <h3>izenburua</h3>
    <pre>Edukia testua</pre>
  </article>
</main>
```

## estilo klasea

### `.Lg`
Txartelaren oinarrizko estiloa. Aplikatu `backdrop-filter` lausotasuna, `border` ertz mikro-erraztua eta `box-shadow` barruko itzala. Aplikatu `svg/glass.svg#h` iragazkia pasatzean.

### `main.Lg`
Orriaren bloke-edukiontzi nagusia. Ertz biribilduak `24px` betegarri sentikorrekin (`16px` zabalera `800px` baino txikiagoa bada, `24px` bestela).

### `.Btn`, `.BtnC`, `.Input`
Oinarrizko osagaien klasearen izena. Estilo hau sartu ondoren, automatikoki lortuko dituzu diseinu malgua, erlazio hierarkikoak eta beira izoztuaren oinarrizko propietateak.