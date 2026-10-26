### Task loop: Run product-management loop for Rhei
**State:** product-run

Run 2 product-management passes. Each pass fans out independent
PM entries, validates them through the smart aggregation state, and implements
the accepted slice with the cheaper implementation target.
