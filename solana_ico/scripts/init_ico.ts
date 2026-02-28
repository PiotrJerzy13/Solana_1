import * as anchor from "@coral-xyz/anchor";
import { Program, BN } from "@coral-xyz/anchor";
import { SolanaIco } from "../target/types/solana_ico";
import {
  createMint,
  getAssociatedTokenAddress,
  createAssociatedTokenAccount,
  mintTo,
  TOKEN_PROGRAM_ID,
  ASSOCIATED_TOKEN_PROGRAM_ID,
} from "@solana/spl-token";

async function main() {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);
  const program = anchor.workspace.SolanaIco as Program<SolanaIco>;
  const admin = provider.wallet as anchor.Wallet;

  console.log("Admin:", admin.publicKey.toString());
  console.log("Program ID:", program.programId.toString());

  // 1. Create mint
  const icoMint = await createMint(
    provider.connection,
    admin.payer,
    admin.publicKey,
    null,
    9
  );
  console.log("ICO Mint:", icoMint.toString());

  // 2. Create admin ATA and mint tokens
  const adminAta = await createAssociatedTokenAccount(
    provider.connection,
    admin.payer,
    icoMint,
    admin.publicKey
  );
  await mintTo(
    provider.connection,
    admin.payer,
    icoMint,
    adminAta,
    admin.publicKey,
    1_000_000 * 10 ** 9
  );
  console.log("Minted 1,000,000 tokens to admin ATA:", adminAta.toString());

  // 3. Derive PDAs
  const [dataPda] = anchor.web3.PublicKey.findProgramAddressSync(
    [Buffer.from("ico_state"), icoMint.toBuffer()],
    program.programId
  );
  const programVaultAta = await getAssociatedTokenAddress(
    icoMint, dataPda, true
  );

  console.log("Data PDA:", dataPda.toString());
  console.log("Vault ATA:", programVaultAta.toString());

  // 4. Initialize ICO
  const now = Math.floor(Date.now() / 1000);
  const startTime = now - 30;
  const endTime = now + 7 * 24 * 3600;

  // In Anchor 0.32, accounts fixed by address constraint are auto-resolved
  // Only pass accounts that are NOT fixed/derived automatically
  await program.methods
    .createIcoAta(
      new BN(500_000),
      new BN(startTime),
      new BN(endTime)
    )
    .accountsPartial({
      icoMint: icoMint,
      icoAtaForIcoProgram: programVaultAta,
      icoAtaForAdmin: adminAta,
      tokenProgram: TOKEN_PROGRAM_ID,
      associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
      systemProgram: anchor.web3.SystemProgram.programId,
    })
    .rpc();

  console.log("\n✅ ICO initialized successfully!");
  console.log("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
  console.log("Copy these into your frontend App.tsx:");
  console.log("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
  console.log(`PROGRAM_ID = '${program.programId.toString()}'`);
  console.log(`ICO_MINT   = '${icoMint.toString()}'`);
  console.log(`ADMIN      = '${admin.publicKey.toString()}'`);
  console.log(`DATA_PDA   = '${dataPda.toString()}'`);
  console.log(`VAULT_ATA  = '${programVaultAta.toString()}'`);
  console.log("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
}

main().catch(console.error);