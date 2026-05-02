#!/usr/bin/env bash
set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color
BOLD='\033[1m'

# ASCII Art
echo -e "${CYAN}${BOLD}"
cat << "EOF"
      __     ___                 
   _ / /___ / _ \__ __ __ __     
  // // -_) |   / // // // /     
 \___/\__/__|_\_\_, / \_,_/      
               /___/             
EOF
echo -e "${NC}"
echo -e "${BOLD}JeRyu Git Compatibility Layer Installer${NC}"
echo -e "==========================================\n"

# Helper functions
command_exists() {
    command -v "$1" >/dev/null 2>&1
}

prompt_install() {
    local cmd=$1
    local install_msg=$2
    local install_cmd=$3

    if ! command_exists "$cmd"; then
        echo -e "${YELLOW}Missing dependency: ${BOLD}$cmd${NC}"
        echo -e "$install_msg"
        while true; do
            read -p "Would you like me to install it for you? (y/n) " yn
            case $yn in
                [Yy]* ) 
                    echo -e "${BLUE}Running: $install_cmd${NC}"
                    eval "$install_cmd"
                    break;;
                [Nn]* ) 
                    echo -e "${RED}Installation aborted. Please install $cmd and run again.${NC}"
                    exit 1;;
                * ) echo "Please answer yes or no.";;
            esac
        done
    else
        echo -e "${GREEN}✓ Found $cmd${NC}"
    fi
}

# Determine OS
OS="$(uname -s)"
echo -e "${BLUE}Detected OS: $OS${NC}"

# Check for Git
if [ "$OS" = "Darwin" ]; then
    prompt_install "git" "Git is required for JeRyu to wrap your version control." "brew install git"
elif command_exists apt-get; then
    prompt_install "git" "Git is required for JeRyu to wrap your version control." "sudo apt-get update && sudo apt-get install -y git"
elif command_exists dnf; then
    prompt_install "git" "Git is required for JeRyu to wrap your version control." "sudo dnf install -y git"
else
    prompt_install "git" "Git is required for JeRyu to wrap your version control." "echo 'Please install git manually'"
fi

# Check for build tools (C compiler / pkg-config)
if [ "$OS" = "Darwin" ]; then
    # Xcode command line tools usually suffice for Mac
    if ! xcode-select -p >/dev/null 2>&1; then
        echo -e "${YELLOW}Missing macOS Command Line Tools (required for building).${NC}"
        while true; do
            read -p "Would you like me to install them? (y/n) " yn
            case $yn in
                [Yy]* ) xcode-select --install; echo "Please re-run this script after installation completes."; exit 0;;
                [Nn]* ) exit 1;;
                * ) echo "Please answer yes or no.";;
            esac
        done
    else
        echo -e "${GREEN}✓ Found build tools${NC}"
    fi
elif command_exists apt-get; then
    prompt_install "gcc" "Build essentials are required to compile JeRyu." "sudo apt-get update && sudo apt-get install -y build-essential pkg-config libssl-dev"
elif command_exists dnf; then
    prompt_install "gcc" "Build essentials are required to compile JeRyu." "sudo dnf groupinstall -y 'Development Tools' && sudo dnf install -y pkgconf-pkg-config openssl-devel"
fi

# Check for Rust and Cargo
if ! command_exists rustc || ! command_exists cargo; then
    echo -e "${YELLOW}Missing dependency: ${BOLD}Rust / Cargo${NC}"
    echo -e "JeRyu is built in Rust. You need the Rust toolchain to compile it."
    while true; do
        read -p "Would you like me to install Rust via rustup? (y/n) " yn
        case $yn in
            [Yy]* ) 
                echo -e "${BLUE}Running rustup installer...${NC}"
                curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
                source "$HOME/.cargo/env"
                break;;
            [Nn]* ) 
                echo -e "${RED}Installation aborted. Please install Rust and run again.${NC}"
                exit 1;;
            * ) echo "Please answer yes or no.";;
        esac
    done
else
    echo -e "${GREEN}✓ Found Rust Toolchain${NC}"
fi

echo -e "\n${BOLD}Building JeRyu release binary...${NC}"
cargo build --release

echo -e "\n${BOLD}Where would you like to install JeRyu?${NC}"
echo "1) Globally (/usr/local/bin) - Requires sudo"
echo "2) Locally (~/.cargo/bin) - Recommended if ~/.cargo/bin is in your PATH"
while true; do
    read -p "Select option (1/2): " choice
    case $choice in
        1 ) 
            INSTALL_DIR="/usr/local/bin"
            echo -e "${BLUE}Installing to $INSTALL_DIR (will ask for sudo password if needed)${NC}"
            sudo mkdir -p $INSTALL_DIR
            sudo cp target/release/jeryu $INSTALL_DIR/jeryu
            sudo chmod +x $INSTALL_DIR/jeryu
            break;;
        2 ) 
            INSTALL_DIR="$HOME/.cargo/bin"
            echo -e "${BLUE}Installing to $INSTALL_DIR${NC}"
            mkdir -p $INSTALL_DIR
            cp target/release/jeryu $INSTALL_DIR/jeryu
            chmod +x $INSTALL_DIR/jeryu
            break;;
        * ) echo "Please enter 1 or 2.";;
    esac
done

echo -e "\n${GREEN}${BOLD}✅ JeRyu installed successfully to $INSTALL_DIR/jeryu!${NC}"
echo -e "\n${CYAN}==========================================${NC}"
echo -e "${BOLD}Next Steps for Git Integration${NC}"
echo -e "${CYAN}==========================================${NC}"
echo -e "To use JeRyu as a seamless Git compatibility layer, add this shell shim to your config."
echo -e "This allows you to type 'git' as normal, but it will flow through JeRyu's AI magic."
echo -e ""
echo -e "Add the following to your ${BOLD}~/.bashrc${NC} or ${BOLD}~/.zshrc${NC}:"
echo -e ""
echo -e "${YELLOW}git() {"
echo -e "    if command -v jeryu >/dev/null 2>&1; then"
echo -e "        command jeryu git \"\$@\""
echo -e "    else"
echo -e "        command git \"\$@\""
echo -e "    fi"
echo -e "}${NC}"
echo -e ""
echo -e "After adding, run ${BOLD}'source ~/.bashrc'${NC} or ${BOLD}'source ~/.zshrc'${NC}."
echo -e "Then type ${BOLD}git status${NC} to see the magic!"
