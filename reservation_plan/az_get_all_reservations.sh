#!/bin/bash
# Example working with bash duplicated in rust.

echo "Fetching all active Azure reservations..."
az reservations reservation-order list \
  --query "[?provisioningState=='Succeeded' || provisioningState=='Active'].name" \
  -o tsv 2>/dev/null | \
while read order_id; do
  az reservations reservation list \
    --reservation-order-id "$order_id" \
    --query "[].{
      ReservationOrderId: '$order_id',
      ReservationId: name,
      DisplayName: properties.displayName,
      SKU: sku.name,
      Quantity: properties.quantity,
      PurchaseDate: properties.purchaseDate,
      ExpiryDate: properties.expiryDate,
      Term: properties.term,
      State: properties.displayProvisioningState,
      Scope: properties.appliedScopeType,
      InstanceFlexibility: properties.instanceFlexibility,
      Type: properties.reservedResourceType,
      BillingPlan: properties.billingPlan,
      Region: location
    }" \
    -o json 2>/dev/null
done | jq -s 'add' > /tmp/all_reservations.json

echo "Saved to /tmp/all_reservations.json"
cat /tmp/all_reservations.json | jq -r '
  ["OrderId", "DisplayName", "SKU", "Qty", "Type", "Purchase", "Expiry", "Term", "State", "Flex"],
  (.[] | [
    .ReservationOrderId[0:8],
    .DisplayName,
    .SKU,
    .Quantity,
    .Type,
    .PurchaseDate,
    .ExpiryDate,
    .Term,
    .State,
    .InstanceFlexibility
  ]) | @tsv
' | column -t -s $'\t'
