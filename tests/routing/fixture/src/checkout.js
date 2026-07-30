import { getUser } from "./users.js";
import { priceCart } from "./pricing.js";

export async function checkout(req) {
  const user = await getUser(req.userId);
  const total = priceCart(req.cart);
  if (total > 100000) throw new Error("cart too large");
  await chargeCard(user.cardToken, total);
  return { ok: true, total };
}

async function chargeCard(token, amount) {
  const res = await fetch("https://payments.internal/charge", {
    method: "POST",
    body: JSON.stringify({ token, amount }),
  });
  return res.json();
}
