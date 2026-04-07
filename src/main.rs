use std::error::Error;
use chrono::{DateTime, FixedOffset, TimeDelta};
use clap::builder::PossibleValue;
use clap::crate_version;

#[derive(Default, Debug, Clone)]
struct ArgumentFlags {
    input_file: String,
    has_headers: bool,                    // -H
    output_header: bool,                  // -p --print-header
    flexible_record: bool,                // -f --flexible
    selected_column: usize,               // -c --column
    insert_position: String,              // -i
    action_to_rfc3339: bool,              // -r --rfc3339
    action_to_utc: bool,                  // -u --utc
    action_to_local: bool,                // -l --local
    action_split: bool,                   // -s --split
    action_remove: bool,                  // -R --remove
    action_duration: bool,                // -d --duration
    // action_offset: bool,                  // -O --offset
    whitespace_trim: String,              // --trim
    input_delimiter: Option<char>,        // --separator
    input_quotes: bool,                   // --single-quotes
    read_as_comment: Option<char>,        // --comment
    output_delimiter: Option<char>,       // --delimiter
    output_quotes: String,                // --quote
}

fn main() {
    let mut args = ArgumentFlags::default();
    let arguments = arguments(args);
    match arguments {
        Ok(arguments) => args=arguments,
        Err(err) => {
            eprintln!("\nError: {}", err);
            std::process::exit(100);
        },
    }

    let execute = reader_writer(args);
    match execute {
        Ok(_) => (),
        Err(err) => {
            eprintln!("\n{}", err);
            std::process::exit(1);
        }
    }
}

fn reader_writer(args:ArgumentFlags) -> Result<(), Box<dyn Error>> {
    let mut reader_builder = csv::ReaderBuilder::new();
    reader_builder.has_headers(args.has_headers).flexible(args.flexible_record);

    let whitespace_trim = args.whitespace_trim.clone();
    match whitespace_trim.as_str() {
        "fields"|"field"|"fiel"|"fie"|"fi"|"f" => {
            reader_builder.trim(csv::Trim::Fields);
        }, 
        "headers"|"header"|"heade"|"head"|"hea"|"he"|"h" =>  {
            reader_builder.trim(csv::Trim::Headers);
        },
        "none"|"non"|"no"|"n" => {
            reader_builder.trim(csv::Trim::None);
        },
        "all"|"al"|"a" => {
            reader_builder.trim(csv::Trim::All);
        },
        _ => {},
    }        

    if let Some(value) = args.input_delimiter {
        reader_builder.delimiter(value as u8);
    } else {
        reader_builder.delimiter(b',');
    }

    if args.input_quotes {
        reader_builder.quote(b'\'');
    } else {
        reader_builder.quote(b'"');
    }

    let mut writer_builder = csv::WriterBuilder::new();
    writer_builder.has_headers(args.has_headers).flexible(args.flexible_record);

    if let Some(value) = args.output_delimiter {
        writer_builder.delimiter(value as u8);
    } else {
        writer_builder.delimiter(b',');
    }

    let output_quotes = args.output_quotes.clone();    
    match output_quotes.as_str() {
        "always"|"alway"|"alwa"|"alw"|"al"|"a" => {
            writer_builder.quote_style(csv::QuoteStyle::Always);
        },
        "never"|"neve"|"nev" => {
            writer_builder.quote_style(csv::QuoteStyle::Never);
        },
        "nonnumeric"|"nonnumeri"|"nonnumer"|"nonnume"|"nonnum"|"nonnu"|"nonn"|"non"|"no" => {
            writer_builder.quote_style(csv::QuoteStyle::NonNumeric);
        },
        "necessary"|"necessar"|"necessa"|"necess"|"neces"|"nece"|"nec" => {
            writer_builder.quote_style(csv::QuoteStyle::Necessary);
        },
        _ => {},
    }
    
    let mut writer = writer_builder.from_writer(std::io::stdout());

    match args.input_file.is_empty() {
        true => {
            let mut reader = reader_builder.from_reader(std::io::stdin());
            let mut dt_cache: chrono::DateTime<FixedOffset> = Default::default();

            if args.has_headers {
                let headers = reader.headers()?;
                let new_headers = process_headers(headers.clone(),args.clone())?;
                reader.set_headers(new_headers.clone());
                // println!("debug: received from process_headers {:?}", reader.headers());
                if args.output_header {
                    writer.write_record(&new_headers)?;
                }
            }

            for record in reader.records() {
                match record {
                    Ok(record) => {
                        let (new_record, new_dt_cache) = process_record(record, args.clone(), dt_cache)?;
                        writer.write_record(&new_record)?;
                        dt_cache = new_dt_cache;
                    },
                    Err(err) => return Err(err.into()),
                }
            }
        }
        false => {
            let mut reader = reader_builder.from_path(args.input_file.clone())?;
            let mut dt_cache: chrono::DateTime<FixedOffset> = Default::default();

            if args.has_headers {
                let headers = reader.headers()?;
                let new_headers = process_headers(headers.clone(),args.clone())?;
                reader.set_headers(new_headers.clone());
                // println!("debug: received from process_headers {:?}", reader.headers());
                if args.output_header {
                    writer.write_record(&new_headers)?;
                }
            }

            for record in reader.records() {
                match record {
                    Ok(record) => {
                        let (new_record, new_dt_cache) = process_record(record, args.clone(), dt_cache)?;
                        writer.write_record(&new_record)?;
                        dt_cache = new_dt_cache;
                    },
                    Err(err) => return Err(err.into()),
                }
            }
        }
        
    }

    writer.flush()?;
    Ok(())
}

fn process_headers(headers:csv::StringRecord, args:ArgumentFlags) -> Result<csv::StringRecord, Box<dyn Error>> {
    let mut vecbuff: Vec<&str> = Default::default();
    
    for field in headers.iter() {
        vecbuff.push(field);
    }

    let column = args.selected_column;
    let postition = args.insert_position.clone(); 

    if args.action_split {
        match postition.as_str() {
            "before"|"befor"|"befo"|"bef"|"be"|"b" => {
                vecbuff.insert(column, "split_time>");
                vecbuff.insert(column, "split_date>");
            },
            "replace"|"replac"|"repla"|"repl"|"rep"|"re"|"r" => {
                vecbuff[column] = "split_time";
                vecbuff.insert(column, "split_date");
            },
            "after"|"afte"|"aft"|"af"|"a" => {
                vecbuff.insert(column+1, "<split_time");
                vecbuff.insert(column+1, "<split_date");
            },
            _ => unreachable!(),
        }
    let new_headers = csv::StringRecord::from(vecbuff);
    return Ok(new_headers);
    }

    if args.action_to_rfc3339 {
        match postition.as_str() {
            "before"|"befor"|"befo"|"bef"|"be"|"b" => {
                vecbuff.insert(column, "to_rfc3339>");
            },
            "replace"|"replac"|"repla"|"repl"|"rep"|"re"|"r" => {
                vecbuff[column] = "to_rfc3339";
            },
            "after"|"afte"|"aft"|"af"|"a" => {
                vecbuff.insert(column+1, "<to_rfc3339");
            },
            _ => unreachable!(),
        }
    let new_headers = csv::StringRecord::from(vecbuff);
    return Ok(new_headers);
    }

    if args.action_to_utc {
        match postition.as_str() {
            "before"|"befor"|"befo"|"bef"|"be"|"b" => {
                vecbuff.insert(column, "to_utc>");
            },
            "replace"|"replac"|"repla"|"repl"|"rep"|"re"|"r" => {
                vecbuff[column] = "to_utc";
            },
            "after"|"afte"|"aft"|"af"|"a" => {
                vecbuff.insert(column+1, "<to_utc");
            },
            _ => unreachable!(),
        }
    let new_headers = csv::StringRecord::from(vecbuff);
    return Ok(new_headers);
    }

    if args.action_to_local {
        match postition.as_str() {
            "before"|"befor"|"befo"|"bef"|"be"|"b" => {
                vecbuff.insert(column, "to_local>");
            },
            "replace"|"replac"|"repla"|"repl"|"rep"|"re"|"r" => {
                vecbuff[column] = "to_local";
            },
            "after"|"afte"|"aft"|"af"|"a" => {
                vecbuff.insert(column+1, "<to_local");
            },
            _ => unreachable!(),
        }
    let new_headers = csv::StringRecord::from(vecbuff);
    return Ok(new_headers);
    }

    if args.action_duration {
        match postition.as_str() {
            "before"|"befor"|"befo"|"bef"|"be"|"b" => {
                vecbuff.insert(column, "duration>");
            },
            "replace"|"replac"|"repla"|"repl"|"rep"|"re"|"r" => {
                vecbuff[column] = "duration";
            },
            "after"|"afte"|"aft"|"af"|"a" => {
                vecbuff.insert(column+1, "<duration");
            },
            _ => unreachable!(),
        }
    let new_headers = csv::StringRecord::from(vecbuff);
    return Ok(new_headers);
    }

    if args.action_remove {
        vecbuff.remove(column);
        let new_headers = csv::StringRecord::from(vecbuff);
        return Ok(new_headers);
    }

    let new_headers = csv::StringRecord::from(vecbuff);
    Ok(new_headers)
}

fn process_record(mut record:csv::StringRecord, args:ArgumentFlags, dt_cache:DateTime<FixedOffset>) -> Result<(csv::StringRecord, DateTime<FixedOffset>), Box<dyn Error>> {
    let mut vecbuff: Vec<&str> = Default::default();
    let mut date: String = Default::default();
    let mut time: String = Default::default();
    let mut date_time: String = Default::default();
    let mut duration_string: String = Default::default();
    let mut duration: TimeDelta= Default::default();
    let mut cache_return: DateTime<FixedOffset> = Default::default();

    for field in record.iter() {
        vecbuff.push(field);
    }

    let veclen = vecbuff.len()-1;
    if args.selected_column > veclen {
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Specified column is out of bound")));
    }

    let column = args.selected_column;
    let position = args.insert_position.clone(); 

    if args.action_split {
        match chrono::DateTime::parse_from_rfc3339(vecbuff[column]) {
            Ok(datetime) => {
                date = format!("{}", datetime.date_naive());
                time = format!("{}", datetime.format("%H:%M:%S %Z"));
                match position.as_str() {
                    "before"|"befor"|"befo"|"bef"|"be"|"b" => {
                        vecbuff.insert(column, &time);
                        vecbuff.insert(column, &date);
                    },
                    "replace"|"replac"|"repla"|"repl"|"rep"|"re"|"r" => {
                        vecbuff[column] = &time;
                        vecbuff.insert(column, &date);
                    },
                    "after"|"afte"|"aft"|"af"|"a" => {
                        vecbuff.insert(column+1, &time);
                        vecbuff.insert(column+1, &date);
                    },
                    _ => unreachable!(),
                }
            },
            Err(_) => {
                match position.as_str() {
                    "before"|"befor"|"befo"|"bef"|"be"|"b" => {
                        vecbuff.insert(column, "parse_err>");
                        vecbuff.insert(column, "parse_err>");
                    },
                    "replace"|"replac"|"repla"|"repl"|"rep"|"re"|"r" => {
                        vecbuff[column] = "parse_err";
                        vecbuff.insert(column, "parse_err");
                    },
                    "after"|"afte"|"aft"|"af"|"a" => {
                        vecbuff.insert(column+1, "<parse_err");
                        vecbuff.insert(column+1, "<parse_err");
                    },
                    _ => unreachable!(),
                }
            },
        }
    record = csv::StringRecord::from(vecbuff);
    return Ok((record, cache_return));
    }

    if args.action_to_rfc3339 {
        let timestamp = vecbuff[column].parse::<i64>(); 
        match timestamp {
            Ok(timestamp) => {
                if let Some(datetime) = chrono::DateTime::from_timestamp(timestamp, 0) {
                    date_time = datetime.to_rfc3339();
                    match args.insert_position.as_str() {
                        "before"|"befor"|"befo"|"bef"|"be"|"b" => {
                            vecbuff.insert(column, &date_time);
                        },
                        "replace"|"replac"|"repla"|"repl"|"rep"|"re"|"r" => {
                            vecbuff[column] = &date_time;
                        },
                        "after"|"afte"|"aft"|"af"|"a" => {
                            vecbuff.insert(column+1, &date_time);
                        },
                        _ => unreachable!(),
                    }
                }
            },
            Err(_) => {
                match args.insert_position.as_str() {
                    "before"|"befor"|"befo"|"bef"|"be"|"b" => {
                        vecbuff.insert(column, "parse_err");
                    },
                    "replace"|"replac"|"repla"|"repl"|"rep"|"re"|"r" => {
                        vecbuff[column] = "parse_err";
                    },
                    "after"|"afte"|"aft"|"af"|"a" => {
                        vecbuff.insert(column+1, "parse_err");
                    },
                    _ => unreachable!(),
                }
            },
        }
    record = csv::StringRecord::from(vecbuff);
    return Ok((record, cache_return));
    }

    if args.action_to_utc {
        match chrono::DateTime::parse_from_rfc3339(vecbuff[column]) {
            Ok(datetime) => {
                let datetime_utc = datetime.with_timezone(&chrono::Utc);
                date_time = datetime_utc.to_rfc3339();
                match args.insert_position.as_str() {
                    "before"|"befor"|"befo"|"bef"|"be"|"b" => {
                            vecbuff.insert(column, &date_time);
                    },
                    "replace"|"replac"|"repla"|"repl"|"rep"|"re"|"r" => {
                            vecbuff[column] = &date_time;
                    },
                    "after"|"afte"|"aft"|"af"|"a" => {
                            vecbuff.insert(column+1, &date_time);
                    },
                    _ => unreachable!(),
                }
            },
            Err(_) => {
                match args.insert_position.as_str() {
                    "before"|"befor"|"befo"|"bef"|"be"|"b" => {
                            vecbuff.insert(column, "parse_err>");
                    },
                    "replace"|"replac"|"repla"|"repl"|"rep"|"re"|"r" => {
                            vecbuff[column] = "parse_err";
                    },
                    "after"|"afte"|"aft"|"af"|"a" => {
                            vecbuff.insert(column+1, "<parse_err");
                    },
                    _ => unreachable!(),
                }
            },
        }
    record = csv::StringRecord::from(vecbuff);
    return Ok((record, cache_return));
    }

    if args.action_to_local {
        match chrono::DateTime::parse_from_rfc3339(vecbuff[column]) {
            Ok(datetime) => {
                let datetime_local = datetime.with_timezone(&chrono::Local);
                date_time = datetime_local.to_rfc3339();
                match args.insert_position.as_str() {
                    "before"|"befor"|"befo"|"bef"|"be"|"b" => {
                        vecbuff.insert(column, &date_time);
                    },
                    "replace"|"replac"|"repla"|"repl"|"rep"|"re"|"r" => {
                        vecbuff[column] = &date_time;
                    },
                    "after"|"afte"|"aft"|"af"|"a" => {
                        vecbuff.insert(column+1, &date_time);
                    },
                    _ => unreachable!(),
                }
            },
            Err(_) => {
                match args.insert_position.as_str() {
                    "before"|"befor"|"befo"|"bef"|"be"|"b" => {
                        vecbuff.insert(column, "parse_err>");
                    },
                    "replace"|"replac"|"repla"|"repl"|"rep"|"re"|"r" => {
                        vecbuff[column] = "parse_err";
                    },
                    "after"|"afte"|"aft"|"af"|"a" => {
                        vecbuff.insert(column+1, "<parse_err");
                    },
                    _ => unreachable!(),
                }
            },
        }
    record = csv::StringRecord::from(vecbuff);
    return Ok((record, cache_return));
    }

    if args.action_duration {
        match (chrono::DateTime::parse_from_rfc3339(vecbuff[column]), dt_cache) {
            (Ok(datetime), mut cache_datetime) => {

                // impl cache_datetime checker, if default change to datetime value
                if cache_datetime == chrono::DateTime::<FixedOffset>::default() {
                    cache_datetime = datetime;
                }
                
             
                duration = match datetime.cmp(&cache_datetime) {
                    std::cmp::Ordering::Greater => datetime - cache_datetime,
                    std::cmp::Ordering::Less => cache_datetime - datetime,
                    std::cmp::Ordering::Equal => cache_datetime - datetime,
                };
               
                let d = duration.num_days();
                let h = duration.num_hours() - (duration.num_days() * 24);
                let m = duration.num_minutes() - (duration.num_hours() * 60);
                let s = duration.num_seconds() - (duration.num_minutes() * 60);

                duration_string = format!("{}d{}h{}m{}s", d, h, m, s);
                    match args.insert_position.as_str() {
                        "before"|"befor"|"befo"|"bef"|"be"|"b" => {
                            vecbuff.insert(column, &duration_string);
                        },
                        "replace"|"replac"|"repla"|"repl"|"rep"|"re"|"r" => {
                            vecbuff[column] = &duration_string;
                        },
                        "after"|"afte"|"aft"|"af"|"a" => {
                            vecbuff.insert(column+1, &duration_string);
                        },
                        _ => unreachable!(),
                    }
                    cache_return = datetime;
                },
            (Err(_), _) => {
                match args.insert_position.as_str() {
                     "before"|"befor"|"befo"|"bef"|"be"|"b" => {
                         vecbuff.insert(column, "parse_err");
                     },
                     "replace"|"replac"|"repla"|"repl"|"rep"|"re"|"r" => {
                         vecbuff[column] = "parse_err";
                     },
                     "after"|"afte"|"aft"|"af"|"a" => {
                         vecbuff.insert(column+1, "parse_err");
                     },
                     _ => unreachable!(),
                }
            }
        }
    record = csv::StringRecord::from(vecbuff);
    return Ok((record, cache_return));
    }

    if args.action_remove {
        vecbuff.remove(column);
        record = csv::StringRecord::from(vecbuff);
        return Ok((record, cache_return));
    }

    // record = csv::StringRecord::from(vecbuff);
    Ok((record, cache_return))
}

fn arguments(mut args: ArgumentFlags) -> Result<ArgumentFlags, Box<dyn Error>> {
    const TEMPLATE: &str = include_str!("./help/help_template.txt");
    const ABOUT: &str = include_str!("./help/about.txt");
    const FILE: &str = include_str!("./help/file.txt");
    const HAS_HEADER: &str = include_str!("./help/has_header.txt");
    const PRINT_HEADER: &str = include_str!("./help/print_header.txt");
    const FLEXIBLE: &str = include_str!("./help/flexible.txt");
    const COLUMN: &str = include_str!("./help/column.txt");
    const POSITION: &str = include_str!("./help/position.txt");
    const TRIM: &str = include_str!("./help/trim.txt");
    const SEPARATOR: &str = include_str!("./help/separator.txt");
    const SINGLE_QUOTES: &str = include_str!("./help/single_quotes.txt");
    const COMMENT: &str = include_str!("./help/comment.txt");
    const DELIMITER: &str = include_str!("./help/delimiter.txt");
    const QUOTES: &str = include_str!("./help/quotes.txt");
    const SPLIT: &str = include_str!("./help/split.txt");
    const RFC3339: &str = include_str!("./help/rfc3339.txt");
    const UTC: &str = include_str!("./help/utc.txt");
    const LOCAL: &str = include_str!("./help/local.txt");
    const DURATION: &str = include_str!("./help/duration.txt");
    const REMOVE: &str = include_str!("./help/remove.txt");

    let matches = clap::Command::new("csvdt")
        .about(ABOUT)
        .author("Michael A Jones <yardquit@pm.me>")
        .version(crate_version!())
        .help_template(TEMPLATE)
        .arg(
            clap::Arg::new("input_file")
                .action(clap::ArgAction::Set)
                .value_name("FILE")
                .help(FILE)
                .num_args(1)
                .required(false)
        )

        .arg(
            clap::Arg::new("has_header")
                .action(clap::ArgAction::SetTrue)
                .value_name("has-header")
                .help(HAS_HEADER)
                .short('H')
                .long("has-header")
                .required(false)
        )

        .arg(
            clap::Arg::new("print_header")
                .action(clap::ArgAction::SetTrue)
                .value_name("print-header")
                .help(PRINT_HEADER)
                .short('p')
                .long("print-header")
                .required(false)
                .requires("has_header")
        )

        .arg(
            clap::Arg::new("flexible")
                .action(clap::ArgAction::SetTrue)
                .value_name("flexible-record")
                .help(FLEXIBLE)
                .short('f')
                .long("flexible")
                .required(false)
        )

        .arg(
            clap::Arg::new("column")
                .action(clap::ArgAction::Set)
                .value_name("num")
                .help(COLUMN)
                .short('c')
                .long("column")
                .num_args(1)
                .required(false)
                .requires("read_column_req"),
        )
        .group(clap::ArgGroup::new("read_column_req")
             .args([
                 "action_split",
                 "action_to_rfc3339",
                 "action_to_utc",
                 "action_to_local",
                 "action_duration",
                 "action_remove",
             ])
             .multiple(true),
        )

        .arg(
            clap::Arg::new("position")
                .action(clap::ArgAction::Set)
                .value_name("pos")
                .help(POSITION)
                .short('i')
                .long("insert")
                .num_args(1)
                .ignore_case(true)
                .required(false)
                .default_value("after")
                .value_parser([
                    PossibleValue::new("after"),
                    PossibleValue::new("afte").hide(true),
                    PossibleValue::new("af").hide(true),
                    PossibleValue::new("a").hide(true),
                    PossibleValue::new("before"),
                    PossibleValue::new("befor").hide(true),
                    PossibleValue::new("befo").hide(true),
                    PossibleValue::new("bef").hide(true),
                    PossibleValue::new("be").hide(true),
                    PossibleValue::new("b").hide(true),
                    PossibleValue::new("replace"),
                    PossibleValue::new("replac").hide(true),
                    PossibleValue::new("repla").hide(true),
                    PossibleValue::new("repl").hide(true),
                    PossibleValue::new("rep").hide(true),
                    PossibleValue::new("re").hide(true),
                    PossibleValue::new("r").hide(true),
                ])
                .requires("actions_req"),
        )
        .group(clap::ArgGroup::new("actions_req")
             .args([
                 "action_split",
                 "action_to_rfc3339",
                 "action_to_utc",
                 "action_to_local",
                 "action_duration",
             ])
             .multiple(true),
        )

        .arg(
            clap::Arg::new("action_to_rfc3339")
                .action(clap::ArgAction::SetTrue)
                .value_name("rfc3339")
                .help(RFC3339)
                .short('r')
                .long("rfc3339")
                .required(false)
                .requires("column")
                .conflicts_with_all([
                    "action_to_utc",
                    "action_to_local",
                    "action_split",
                    "action_duration",
                    "action_remove"
                ])
        )

        .arg(
            clap::Arg::new("action_to_utc")
                .action(clap::ArgAction::SetTrue)
                .value_name("utc")
                .help(UTC)
                .short('u')
                .long("utc")
                .required(false)
                .requires("column")
                .conflicts_with_all([
                    "action_to_rfc3339",
                    "action_to_local",
                    "action_split",
                    "action_duration",
                    "action_remove"
                ])
        )

        .arg(
            clap::Arg::new("action_to_local")
                .action(clap::ArgAction::SetTrue)
                .value_name("local")
                .help(LOCAL)
                .short('l')
                .long("local")
                .required(false)
                .requires("column")
                .conflicts_with_all([
                    "action_to_utc",
                    "action_to_rfc3339",
                    "action_split",
                    "action_duration",
                    "action_remove"
                ])
        )

        .arg(
            clap::Arg::new("action_split")
                .action(clap::ArgAction::SetTrue)
                .value_name("split")
                .help(SPLIT)
                .short('s')
                .long("split")
                .required(false)
                .requires("column")
                .conflicts_with_all([
                    "action_to_utc",
                    "action_to_local",
                    "action_to_rfc3339",
                    "action_duration",
                    "action_remove"
                ])
        )

        .arg(
            clap::Arg::new("action_duration")
                .action(clap::ArgAction::SetTrue)
                .value_name("duration")
                .help(DURATION)
                .short('d')
                .long("duration")
                .required(false)
                .requires("column")
                .conflicts_with_all([
                    "action_to_utc",
                    "action_to_local",
                    "action_to_rfc3339",
                    "action_split",
                    "action_remove"
                ])
        )

        .arg(
            clap::Arg::new("action_remove")
                .action(clap::ArgAction::SetTrue)
                .value_name("remove")
                .help(REMOVE)
                .short('R')
                .long("remove")
                .required(false)
                .requires("column")
                .conflicts_with_all([
                    "action_to_utc",
                    "action_to_local",
                    "action_to_rfc3339",
                    "action_split",
                    "action_duration",
                ])
        )

        .arg(
            clap::Arg::new("trim")
                .action(clap::ArgAction::Set)
                .value_name("trim")
                .help(TRIM)
                .long("trim")
                .num_args(1)
                .required(false)
                .ignore_case(true)
                .default_value("all")
                .value_parser([
                    PossibleValue::new("all"),
                    PossibleValue::new("al").hide(true),
                    PossibleValue::new("a").hide(true),
                    PossibleValue::new("fields"),
                    PossibleValue::new("field").hide(true),
                    PossibleValue::new("fiel").hide(true),
                    PossibleValue::new("fie").hide(true),
                    PossibleValue::new("fi").hide(true),
                    PossibleValue::new("f").hide(true),
                    PossibleValue::new("headers"),
                    PossibleValue::new("header").hide(true),
                    PossibleValue::new("heade").hide(true),
                    PossibleValue::new("head").hide(true),
                    PossibleValue::new("hea").hide(true),
                    PossibleValue::new("he").hide(true),
                    PossibleValue::new("h").hide(true),
                    PossibleValue::new("none"),
                    PossibleValue::new("non").hide(true),
                    PossibleValue::new("no").hide(true),
                    PossibleValue::new("n").hide(true),
                ]),
        )

        .arg(
            clap::Arg::new("separator")
                .action(clap::ArgAction::Set)
                .value_name("char")
                .help(SEPARATOR)
                .long("separator")
                .num_args(1)
                .required(false)
        )

        .arg(
            clap::Arg::new("single_quotes")
                .action(clap::ArgAction::SetTrue)
                .value_name("single-quote")
                .help(SINGLE_QUOTES)
                .long("single-quote")
                .required(false)
        )

        .arg(
            clap::Arg::new("comment")
                .action(clap::ArgAction::Set)
                .value_name("char")
                .help(COMMENT)
                .long("comment")
                .num_args(1)
                .required(false)
        )

        .arg(
            clap::Arg::new("delimiter")
                .action(clap::ArgAction::Set)
                .value_name("char")
                .help(DELIMITER)
                .long("delimiter")
                .num_args(1)
                .required(false)
        )

        .arg(
            clap::Arg::new("quotes")
                .action(clap::ArgAction::Set)
                .value_name("quoting")
                .help(QUOTES)
                .long("quote")
                .alias("quote")
                .num_args(1)
                .required(false)
                .ignore_case(true)
                .default_value("necessary")
                .value_parser([
                    PossibleValue::new("always"),    
                    PossibleValue::new("alway").hide(true),    
                    PossibleValue::new("alwa").hide(true),    
                    PossibleValue::new("alw").hide(true),    
                    PossibleValue::new("al").hide(true),    
                    PossibleValue::new("a").hide(true),    
                    PossibleValue::new("necessary"),
                    PossibleValue::new("necessar").hide(true),
                    PossibleValue::new("necessa").hide(true),
                    PossibleValue::new("necess").hide(true),
                    PossibleValue::new("neces").hide(true),
                    PossibleValue::new("nece").hide(true),
                    PossibleValue::new("nec").hide(true),
                    PossibleValue::new("never"),
                    PossibleValue::new("neve").hide(true),
                    PossibleValue::new("nev").hide(true),
                    PossibleValue::new("nonnumeric"),
                    PossibleValue::new("nonnumeri").hide(true),
                    PossibleValue::new("nonnumer").hide(true),
                    PossibleValue::new("nonnume").hide(true),
                    PossibleValue::new("nonnum").hide(true),
                    PossibleValue::new("nonnu").hide(true),
                    PossibleValue::new("nonn").hide(true),
                    PossibleValue::new("non").hide(true),
                    PossibleValue::new("no").hide(true),
                ])
        )

    .get_matches();

    if let Some(value) = matches.get_one::<String>("input_file") {
        args.input_file = value.to_string();
    }

    if let Some(value) = matches.get_one::<String>("trim") {
        args.whitespace_trim = value.to_string();
    }

    if let Some(value) = matches.get_one::<String>("separator") {
        if value.len() > 1 {
            return Err(Box::new(std::io::Error::new(std:: io::ErrorKind::InvalidData,
                "String was provided to 'input-delimiter' but expeced a char")));
        }
        if let Some(cast_to_char) = value.chars().next() {
            args.input_delimiter = Some(cast_to_char);
        }
    }

    if let Some(value) = matches.get_one::<String>("comment") {
        if value.len() > 1 {
            return Err(Box::new(std::io::Error::new(std:: io::ErrorKind::InvalidData,
                "String was provided to 'comment' but expeced a char")));
        }
        if let Some(cast_to_char) = value.chars().next() {
            args.read_as_comment = Some(cast_to_char);
        }
    }    

    if let Some(value) = matches.get_one::<String>("column") {
        let cast_to_usize = value.parse::<usize>()?;
        args.selected_column = cast_to_usize;
    }

    if let Some(value) = matches.get_one::<String>("delimiter") {
        if value.len() > 1 {
            return Err(Box::new(std::io::Error::new(std:: io::ErrorKind::InvalidData,
                "String was provided to 'output-delimiter' but expeced a char")));
        }
        if let Some(cast_to_char) = value.chars().next() {
            args.output_delimiter = Some(cast_to_char);
        }
    }    

    if let Some(value) = matches.get_one::<String>("quotes") {
            args.output_quotes = value.to_lowercase().to_string();
    }    

    if let Some(value) = matches.get_one::<String>("position") {
            args.insert_position = value.to_lowercase().to_string();
    }    

    args.has_headers = matches.get_flag("has_header");
    args.output_header = matches.get_flag("print_header");
    args.input_quotes = matches.get_flag("single_quotes");
    args.flexible_record = matches.get_flag("flexible");
    args.action_to_rfc3339 = matches.get_flag("action_to_rfc3339");
    args.action_to_utc = matches.get_flag("action_to_utc");
    args.action_to_local = matches.get_flag("action_to_local");
    args.action_split = matches.get_flag("action_split");
    args.action_duration = matches.get_flag("action_duration");
    args.action_remove = matches.get_flag("action_remove");
    Ok(args)
}
