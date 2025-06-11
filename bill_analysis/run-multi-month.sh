#!/bin/bash
# set -e
set -u
debug=0
# Starting year and month
start_year=2024
start_month=03

# Ending year and month
# Detect OS
OS=$(uname -s)

# Get year and previous month as a number
if [ "$OS" = "Linux" ]; then
    # Linux (GNU date)
    end_month=$(date --date="last month" +%m)
    end_year=$(date --date="last month" +%Y)
elif [ "$OS" = "Darwin" ]; then
    # macOS (BSD date)
    end_month=$(date -v-1m +%m)
    end_year=$(date -v-1m +%Y)
else
    echo "Unsupported OS: $OS"
    exit 1
fi

# Convert to a comparable number (YYYYMM)
start=$((start_year * 100 + start_month))
end=$((end_year * 100 + end_month))
current=$start

cargo build --release

echo
echo "# Processing months from start=$start to end=$end"
echo '"Total cost","res_save","res_unused","no_reservation","err"'
# Loop through each month from start to end
while [ $current -le $end ]; do
    # Format the date as YYYYMM
    year=$((current / 100))
    month=$((current % 100))
    formatted_date=$(printf "%04d%02d" $year $month)

    # Run the cargo command
    # echo "Running for $formatted_date..."
    ./target/release/bill_analysis --bill-path "./csv_data/$formatted_date" | \
        grep "Total cost NZ" | \
        grep -v "date:" | \
        awk -F'NZ\\$' -v date="$formatted_date" '{
            # Clean each field: remove commas, labels, and extra characters
            gsub(/[,=]/, "", $2); gsub(/[^0-9.]/, "", $2);  # Total cost
            gsub(/[,=]/, "", $3); gsub(/[^0-9.]/, "", $3);  # res_save
            gsub(/[,=]/, "", $4); gsub(/[^0-9.]/, "", $4);  # res_unused
            gsub(/[,=]/, "", $5); gsub(/[^0-9.]/, "", $5);  # no_reservation
            gsub(/[,=]/, "", $6); gsub(/[^0-9.]/, "", $6);  # err
            print date ", " $2 ", " $3 ", " $4 ", " $5 ", " $6
        }' 
        # | \
        # sed 's/ //g'
    
    # echo "Finished processing $formatted_date exited with code $?"

    # Increment month
    month=$((month + 1))
    if [ $month -gt 12 ]; then
        month=1
        year=$((year + 1))
    fi
    current=$((year * 100 + month))
done
echo "# All months processed. start=$start to end=$end"
