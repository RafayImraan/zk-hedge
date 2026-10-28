import "./globals.css";

export const metadata = {
  title: "ZK-Hedge",
  description: "Swap from Zcash Testnet to Arbitrum Sepolia",
};

export default function RootLayout({ children }) {
  return (
    <html lang="en">
      <body>{children}</body>
    </html>
  );
}
