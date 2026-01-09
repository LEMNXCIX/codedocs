/** @type {import('tailwindcss').Config} */
module.exports = {
  darkMode: 'class',
  content: ["./index.html", "./src/**/*.rs"],
  theme: {
    extend: {
      colors: {
        brand: {
          dark: "#121212",
          blue: "#4fc3f7",
          orange: "#ffb74d",
        },
      },
    },
  },
  plugins: [],
}
