export function priceCart(cart) {
  let total = 0;
  for (const line of cart) total += line.qty * line.unitPrice;
  if (cart.length > 10) total = total * 0.95;
  if (cart.promoCode) total = applyPromo(total, cart.promoCode);
  return Math.round(total);
}

function applyPromo(total, code) {
  const pct = PROMOS[code];
  return pct ? total - total * pct : total;
}
const PROMOS = { SAVE10: 0.1, SAVE20: 0.2 };
