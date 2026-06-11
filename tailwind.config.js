/** @type {import('tailwindcss').Config} */
module.exports = {
  darkMode: 'class',
  content: ["./index.html", "./src/**/*.rs"],
  theme: {
    extend: {
      colors: {
        base: {
          50: "#FCFFFF",
          100: "#F4F5F5",
          200: "#E6E7E7",
          300: "#D0D1D1",
          400: "#A3A4A4",
          500: "#767777",
          600: "#5F6060",
          700: "#4A4B4B",
          800: "#3D3E3E",
          900: "#2F2F2F",
        },
        brand: {
          dark: "#2F2F2F",
          orange: "#ffb74d",
          blue: "#4fc3f7",
        },
      },
      fontFamily: {
        unifraktur: ['UnifrakturMaguntia', 'serif'],
      },
    },
  },
  plugins: [],
}
