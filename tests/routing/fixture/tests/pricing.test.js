import test from "node:test";
import assert from "node:assert";
import { priceCart } from "../src/pricing.js";

test("prices a cart", () => {
  assert.equal(priceCart([{ qty: 2, unitPrice: 500 }]), 1000);
});
