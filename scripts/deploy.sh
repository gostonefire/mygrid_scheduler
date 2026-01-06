#!/bin/bash

REPO_NAME=$1
DEV_DIR=$2
SUB_SCRIPT_LOG=$3

HOME="/home/petste"
APP_DIR="MyGridScheduler"

if [ -z "$REPO_NAME" ] || [ -z "$DEV_DIR" ] || [ -z "$SUB_SCRIPT_LOG" ]; then
  echo "Usage: $0 <repo_name> <dev_dir> <sub_script_log>"
  exit 1
fi

# Get the owner of the DEV_DIR
DEV_USER=$(stat -c '%U' "$DEV_DIR")

# Function containing the logic to be run as the directory owner
run_as_user() {
  mkdir -p "$HOME/$APP_DIR/config" >> "$SUB_SCRIPT_LOG" 2>&1
  EXIT_CODE=$?
  if [ $EXIT_CODE -ne 0 ]; then
    echo "could not create $HOME/$APP_DIR/config..."
    exit $EXIT_CODE
  fi

  mkdir -p "$HOME/$APP_DIR/logs" >> "$SUB_SCRIPT_LOG" 2>&1
  EXIT_CODE=$?
  if [ $EXIT_CODE -ne 0 ]; then
    echo "could not create $HOME/$APP_DIR/logs..."
    exit $EXIT_CODE
  fi

  mkdir -p "$HOME/$APP_DIR/last_version" >> "$SUB_SCRIPT_LOG" 2>&1
  EXIT_CODE=$?
  if [ $EXIT_CODE -ne 0 ]; then
    echo "could not create $HOME/$APP_DIR/last_version..."
    exit $EXIT_CODE
  fi

  # shellcheck disable=SC2164
  cd "$DEV_DIR"/"$REPO_NAME" >> "$SUB_SCRIPT_LOG" 2>&1
  EXIT_CODE=$?
  if [ $EXIT_CODE -ne 0 ]; then
    echo "could not change directory to $DEV_DIR"/"$REPO_NAME..."
    exit $EXIT_CODE
  fi

  if [ -f "$HOME/$APP_DIR/$REPO_NAME" ]; then
    mv "$HOME/$APP_DIR/$REPO_NAME" "$HOME/$APP_DIR/last_version/"
  fi

  cp "./target/release/$REPO_NAME" "$HOME/$APP_DIR/" >> "$SUB_SCRIPT_LOG" 2>&1
  EXIT_CODE=$?
  if [ $EXIT_CODE -ne 0 ]; then
    echo "could not copy ./target/release/$REPO_NAME to $HOME/$APP_DIR/..."
    exit $EXIT_CODE
  fi

  ### Add any extra deploy features to be run as dev user from here ###
  mkdir -p "$HOME/$APP_DIR/base_data"
  EXIT_CODE=$?
  if [ $EXIT_CODE -ne 0 ]; then
    echo "could not create $HOME/$APP_DIR/base_data..."
    exit $EXIT_CODE
  fi

  mkdir -p "$HOME/$APP_DIR/schedule"
  EXIT_CODE=$?
  if [ $EXIT_CODE -ne 0 ]; then
    echo "could not create $HOME/$APP_DIR/schedule..."
    exit $EXIT_CODE
  fi

  cp "./cron/start.sh" "$HOME/$APP_DIR/" >> "$SUB_SCRIPT_LOG" 2>&1
  EXIT_CODE=$?
  if [ $EXIT_CODE -ne 0 ]; then
    echo "could not copy ./cron/start.sh to $HOME/$APP_DIR/..."
    exit $EXIT_CODE
  fi

  chmod 755 "$HOME/$APP_DIR/start.sh"

  ### until to here! ##################################################
}

########## From here the script will be run as root, so add any commands such as systemctl etc. from here ##########

########## until to here! ##########################################################################################

# Export variables so the subshell can see them, then run the function as the owner
export REPO_NAME DEV_DIR HOME APP_DIR SUB_SCRIPT_LOG SERVICE_NAME

sudo -u "$DEV_USER" -E bash -c "$(declare -f run_as_user); run_as_user"
EXIT_CODE=$?
if [ $EXIT_CODE -ne 0 ]; then
  exit $EXIT_CODE
fi

########## From here the script will be run as root, so add any commands such as systemctl etc. from here ##########

########## until to here! ##########################################################################################
