import json
import httpx
from Cryptodome.Cipher import AES
from Cryptodome.Util.Padding import unpad
import xml.etree.ElementTree as ET

def extract_dlf_token(data: bytes) -> str:
    key = bytes([65,50,114,45,208,130,239,176,220,100,87,197,118,104,202,9])
    iv = bytes(16)
    
    def try_decrypt(d):
        try:
            cipher = AES.new(key, AES.MODE_CBC, iv)
            decrypted = unpad(cipher.decrypt(d), AES.block_size)
            return decrypted.decode('utf-8').lstrip('\xfe\xff')
        except Exception:
            return None
            
    xml_str = try_decrypt(data)
    if not xml_str and len(data) > 0x41:
        xml_str = try_decrypt(data[0x41:])
        
    if xml_str:
        root = ET.fromstring(xml_str)
        ns = {'ns': 'http://ea.com/license'}
        token_node = root.find('.//ns:GameToken', ns)
        if token_node is not None:
            return token_node.text.strip().replace(' ', '').replace('\n', '').replace('\r', '')
    return None

def generate_token_via_api(ticket: str, access_token: str, machine_hash: str):
    ticket_raw, engine, content_id = ticket.split('|')
    
    params = {
        'contentId': content_id,
        'machineHash': machine_hash,
        'ea_eadmtoken': access_token,
        'requestToken': ticket_raw,
        'requestType': engine
    }

    headers = {
        'User-Agent': 'EACTransaction',
        'X-Requester-Id': 'Origin Online Activation'
    }
    
    with httpx.Client() as client:
        res = client.get('https://proxy.novafusion.ea.com/licenses', headers=headers, params=params)
        
        if res.status_code == 200 and res.headers.get('content-type', '').startswith('application/octet-stream'):
            token = extract_dlf_token(res.content)
            if token:
                return token
            else:
                print("Failed to decrypt the DLF response from EA.")
        else:
            print(f"API Error: HTTP {res.status_code}")
            print(res.text)
            
    return None

if __name__ == '__main__':
    # Đọc access token từ cache của tool Anadius
    with open(r'E:\Compressed\phần mềm tạo token cho game denuvo\EA GAME\token_data.txt', 'r', encoding='utf-8') as f:
        token_data = json.load(f)

    access_token = token_data.get('last_access_token', [None, None])[1]
    machine_hash = token_data.get('machine_hash', '')
    
    ticket = 'CgDNEi06KvP_MSvz3zEq80CaQfOGBh8WuOjQjwwB8RfSAs6uz6J-IMP6UmHZpuuVB_xL2Rof40q_B7Rp8jKf75T82v6PJHGMVmyCwsjxF2pooLPaQH_XgHYSUZuJ4R0W9Hq57_-GRjZG5uLXC3TFUu6q8xQl-dAMgsNSbtLt7pZwYpeI0p1025lwktT9IexEAZSJj88SvyBrW7K7LGj7fAzZtkPL7UsQXYAxHSbKWABbtpF6aFW8P2hHszBggFAciwbtCQIyMSxNkzp9CCHSQPOs2ydyTIB5qZm31ijPuUe7-gwJEK7kl9Y9_RLymBWBQNPjt2yG64dlaYt3npvdCsnd-0jaXGeLexSpU2I2POGrJh3XribLDLgG6ww-gEryieSXLkfvR6JF71bx7ffnzN05j2Ot9YdejCMkLF8862g_kzMcPs_virXGbdjbdAO5qYOfMINwo4R5281K98mFwQwgFPyigxeJ8iFFWqnE69j4eqaqh98VF4EAhWiIGUoO5oTiT4KWhGJGCKTaC3Dq0OMJW7F-qiBgLxcrOHUN17mtYetBgJbWSeG0vzVLPlPlbs8aa5pwgVmMp7k7cHFYExkb3uANVCjodH0qWgvCbpZx__yDheDJ-xOD8UqmBfTMBrdW4l3BEcj8ECJJ4VwLCi277_NwZcDwMbbjKPt84dSVp36MIJ7FvCsXLPwim_43e1lcm1gpCH_ZaAKhDrd3X2IfB9tjAj7FchWAKOZ5FypaaARC5GNwUewezmGJGfBN26A2uUG0LEsDuqjYVqJeKK5PFn03s6FkNO4vOvkofx9zvRhLzmZZEtTEm4VUK5OxngWAYX08zZzqQmW0PaqqdgAEwCGTfHkGJJCymw0qbpqbfvQPil-3-NrTBWZAJM08hV-gynKE9kE8-DPFXBsabn6FJxCih7B8olpXqTxdmoZhmZ-R3Gh8T3mzluGnqk3mDsunE4_Kmc8iP4zFwpQgN2azgcFSeXrazII_nYQCG4yK2zZLHfIxsZAsLXpLM056Y6NsPmwj4sc-tGIIOZIErFn95R0B-lwSa7oxQGesARk-osz5f2wbPIFENJhq05OMH602YIJN2KyK90NgjcICOwKe5PKqQKNp5O9SQVrcb-yhOf7T9lbZ_5jnYRAq6HyPmbsZ2Cz9aFPM4UCX5po-vPGrZZp3f9JreownGqcak3AZVP_C2m7Cw20GjEvKuK7Se1Xa4XJcyX51XDN9NwGwUI3hhIcIkmmnbhMGRdpv2FKpeKWilIwCK-MRSVAc6I7RW_UrQ4Ro2dxfnQiorjrs2WwGxRthSfsJHy1hNMaeCwDpdw18R0foHHsb1_qiAnJlvYhVSqlYkRiKu4Wuwi3wcN3i9DfzjkTMAH5yKkCfR_u7PJmIjay-dh648_RiElCfWoLtXVPFCvjiqy1IyRQF1uXStCAaNwML8XWh5kai3sB_HAFhFSqX7Qf3zUvNannu6KDv0YPyg2XLh4Nlbxj9zW9o-53pZ77ZpfGUv-1iulyGJJ3JUbvm1L2T9gFXHmiXYBWEQ7YGbqdnRnD5NF62wRKS1wF2Sx1My7JfhBKN2QzCs9NFd75uWFKW60E8SWbfmlAKbKIuYX3s2gJGaDO4B-80s5_x5EpP-SYtlKWXUhYM14erLHmUr-_rnyBZ9dqeIB9_LRr04rID-IodGJpsZLSRjqVWmAMoZWNtcYP8tf_4B0MaRTl54LNA0tqQCKgm6hi7P5TZ7n5_JPaSwJ-bY1vZ3f9HdRPpv8Bj7XzblvK6IcgQHOS-qfc9b040tLYf8dV4ZxNQNpBqpiWimgaD-J8zHdDrhdHDrdQ-UyIYM-mOnSoa9UOgWWt5r8MSPVWZDub_cRpyOs6IR6mopg3HJCN3IOhM7Ln5QKlK2h9d_J6o7Epn8k3FNZIviwGp3cM4OwZNRMxEvZMlWFXuHycRw7z_3d4=|0|16425677'
    
    print("Sending API Request to EA...")
    token = generate_token_via_api(ticket, access_token, machine_hash)
    
    if token:
        print("\n[SUCCESS] Generated Token Length:", len(token))
        # Ghi ra file cho user
        out_path = r'E:\FC.26.NOTACRACK\DenuvoToken_Output.txt'
        with open(out_path, 'w', encoding='utf-8') as f:
            f.write(token)
        print(f"Full token has been written to: {out_path}")
