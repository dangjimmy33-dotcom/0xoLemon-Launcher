-- Example lua file for Hello Kitty Island Adventure
-- Place this in: C:\Program Files (x86)\Steam\config\stplug-in\2495100.lua

-- Add main game AppID (required)
addappid(2495100)

-- Add depot with decryption key (if you have it)
-- Format: adddepot(depotId, "HEX_KEY_HERE")
-- Example (this is a fake key - replace with real one):
-- adddepot(2495101, "AABBCCDDEEFF00112233445566778899AABBCCDDEEFF00112233445566778899")

-- Add DLCs (optional)
-- adddlc(2495100, 2495200)  -- Example DLC
-- adddlc(2495100, 2495201)  -- Another DLC

print("Loaded Hello Kitty Island Adventure")
